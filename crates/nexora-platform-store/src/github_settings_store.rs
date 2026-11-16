use crate::{
    GitNetworkPolicy, PlatformStore, ProviderSecretContext, ProviderSecretEnvelope,
    audit_writer::AuditWriter, github_client::GithubClient, github_secrets::GithubSecrets,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
use zeroize::Zeroizing;
pub(crate) struct GithubSettingsStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> GithubSettingsStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn metadata(&mut self, ctx: &AuthContext) -> Result<GithubConfiguration, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::InstallationManage, None)?;
        let row = self
            .store
            .client
            .query_opt(
                "SELECT to_jsonb(s)-'secrets'-'singleton' FROM github_settings s WHERE singleton",
                &[],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        PlatformStore::decode(row.get(0))
    }
    pub fn context(id: &str, host: &str, purpose: &str, revision: i64) -> ProviderSecretContext {
        ProviderSecretContext {
            binding_id: id.into(),
            provider: GitProvider::Github.into(),
            host: host.into(),
            purpose: purpose.into(),
            revision,
        }
    }
    pub fn configure(
        &mut self,
        ctx: &AuthContext,
        input: &GithubAppRegistration,
        policy: &GitNetworkPolicy,
    ) -> Result<GithubConfiguration, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::InstallationManage, None)?;
        policy.validate()?;
        if input
            .app_id
            .parse::<u64>()
            .ok()
            .filter(|v| *v > 0)
            .map(|v| v.to_string())
            .as_deref()
            != Some(input.app_id.as_str())
            || input.client_id.is_empty()
            || input.client_id.len() > 512
            || input.client_secret.is_empty()
            || input.client_secret.len() > 8192
            || input.private_key_pem.len() > 16384
            || !(32..=512).contains(&input.webhook_secret.len())
            || input.expected_revision < 0
            || input.redirect_uri
                != format!("{}/api/v1/git/github/callback", policy.callback_origin)
        {
            return Err(PlatformError::Validation("invalid GitHub App registration"));
        }
        let host = policy.github_host()?;
        let cipher = self.store.provider_secret_cipher()?;
        let request = self.store.request_context.clone();
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "github.configure",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("{}:github-settings", self.store.namespace)],
        )
        .map_err(PlatformStore::database_error)?;
        let previous=tx.query_opt("SELECT revision,app_id,client_id,origin,api_origin,credential_revision FROM github_settings WHERE singleton FOR UPDATE",&[]).map_err(PlatformStore::database_error)?;
        let revision = previous.as_ref().map_or(0, |r| r.get::<_, i64>(0));
        if revision != input.expected_revision {
            return Err(PlatformError::Conflict(
                "GitHub configuration revision changed",
            ));
        }
        let app = GithubClient::new(policy).app(&input.client_id, &input.private_key_pem)?;
        if GithubClient::id(&app["id"])? != input.app_id {
            return Err(PlatformError::Forbidden);
        }
        GithubClient::app_permissions(&app["permissions"])?;
        let slug = app["slug"]
            .as_str()
            .filter(|v| {
                !v.is_empty()
                    && v.len() <= 160
                    && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
            .ok_or(PlatformError::Unavailable)?;
        let secrets = GithubSecrets {
            client_secret: input.client_secret.clone(),
            private_key_pem: input.private_key_pem.clone(),
            webhook_secret: input.webhook_secret.clone(),
        };
        let plain =
            Zeroizing::new(serde_json::to_vec(&secrets).map_err(|_| PlatformError::Storage)?);
        let next = revision + 1;
        let credential_revision = previous.as_ref().map_or(0, |row| row.get::<_, i64>(5)) + 1;
        let encrypted = serde_json::to_value(cipher.seal(
            &Self::context(
                "github-settings",
                &host,
                "github_app_secrets",
                credential_revision,
            ),
            &plain,
        )?)
        .map_err(|_| PlatformError::Storage)?;
        let value:Value=tx.query_one("INSERT INTO github_settings AS s(singleton,app_id,client_id,slug,origin,api_origin,host,redirect_uri,enabled,revision,secrets,credential_revision) VALUES(true,$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(singleton) DO UPDATE SET app_id=EXCLUDED.app_id,client_id=EXCLUDED.client_id,slug=EXCLUDED.slug,origin=EXCLUDED.origin,api_origin=EXCLUDED.api_origin,host=EXCLUDED.host,redirect_uri=EXCLUDED.redirect_uri,enabled=EXCLUDED.enabled,revision=EXCLUDED.revision,secrets=EXCLUDED.secrets,credential_revision=EXCLUDED.credential_revision RETURNING to_jsonb(s)-'secrets'-'singleton'",&[&input.app_id,&input.client_id,&slug,&policy.github_origin,&policy.github_api_origin,&host,&input.redirect_uri,&input.enabled,&next,&encrypted,&credential_revision]).map_err(PlatformStore::database_error)?.get(0);
        let revoke = !input.enabled
            || previous.as_ref().is_some_and(|r| {
                r.get::<_, String>(1) != input.app_id
                    || r.get::<_, String>(2) != input.client_id
                    || r.get::<_, String>(3) != policy.github_origin
                    || r.get::<_, String>(4) != policy.github_api_origin
            });
        if revoke {
            tx.execute("UPDATE git_connections SET status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE provider='github' AND status='active'",&[]).map_err(PlatformStore::database_error)?;
            tx.execute("UPDATE project_repositories SET authorization_status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE connection_id IN (SELECT id FROM git_connections WHERE provider='github') AND authorization_status='authorized'",&[]).map_err(PlatformStore::database_error)?;
        } else {
            tx.execute("UPDATE git_connections SET config_revision=$1 WHERE provider='github' AND status='active'",&[&next]).map_err(PlatformStore::database_error)?;
        }
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "git.configuration_updated",
            Some("github"),
            json!({"revision":next,"enabled":input.enabled}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
    pub fn load(
        &mut self,
        policy: &GitNetworkPolicy,
    ) -> Result<(GithubConfiguration, GithubSecrets), PlatformError> {
        let row=self.store.client.query_opt("SELECT to_jsonb(s)-'secrets'-'singleton',secrets,credential_revision FROM github_settings s WHERE singleton AND enabled",&[]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let config: GithubConfiguration = PlatformStore::decode(row.get(0))?;
        if config.origin != policy.github_origin || config.api_origin != policy.github_api_origin {
            return Err(PlatformError::Conflict(
                "GitHub operator origin changed; reconfigure application",
            ));
        }
        let encrypted: ProviderSecretEnvelope = PlatformStore::decode(row.get(1))?;
        let plain = self.store.provider_secret_cipher()?.open(
            &Self::context(
                "github-settings",
                &config.host,
                "github_app_secrets",
                row.get(2),
            ),
            &encrypted,
        )?;
        let secrets = serde_json::from_slice(&plain).map_err(|_| PlatformError::Storage)?;
        Ok((config, secrets))
    }
}
