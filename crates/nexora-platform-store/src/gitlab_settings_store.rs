use crate::{
    GitNetworkPolicy, PlatformStore, ProviderSecretContext, ProviderSecretEnvelope,
    audit_writer::AuditWriter, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
use zeroize::Zeroizing;
pub(crate) struct GitlabSettingsStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> GitlabSettingsStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn metadata(&mut self, ctx: &AuthContext) -> Result<GitlabConfiguration, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::InstallationManage, None)?;
        let row = self.store.client.query_opt(
            "SELECT to_jsonb(s)-'client_secret'-'singleton' FROM gitlab_settings s WHERE singleton",
            &[],
        ).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        PlatformStore::decode(row.get(0))
    }
    pub fn context(id: &str, host: &str, purpose: &str, revision: i64) -> ProviderSecretContext {
        ProviderSecretContext {
            binding_id: id.into(),
            host: host.into(),
            provider: GitProvider::Gitlab.into(),
            purpose: purpose.into(),
            revision,
        }
    }
    pub fn configure(
        &mut self,
        ctx: &AuthContext,
        input: &GitlabClientRegistration,
        policy: &GitNetworkPolicy,
    ) -> Result<GitlabConfiguration, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::InstallationManage, None)?;
        policy.validate_registration(input)?;
        let host = policy.host()?;
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
            "gitlab.configure",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("{}:gitlab-settings", self.store.namespace)],
        )
        .map_err(PlatformStore::database_error)?;
        let previous = tx
            .query_opt(
                "SELECT revision,client_id,origin,credential_revision FROM gitlab_settings WHERE singleton FOR UPDATE",
                &[],
            )
            .map_err(PlatformStore::database_error)?;
        let revision = previous.as_ref().map_or(0, |r| r.get::<_, i64>(0));
        if revision != input.expected_revision {
            return Err(PlatformError::Conflict(
                "GitLab configuration revision changed",
            ));
        }
        let next = revision + 1;
        let credential_revision = previous.as_ref().map_or(0, |row| row.get::<_, i64>(3)) + 1;
        let context = Self::context(
            "gitlab-settings",
            &host,
            "gitlab_client_secret",
            credential_revision,
        );
        let secret = serde_json::to_value(cipher.seal(&context, input.client_secret.as_bytes())?)
            .map_err(|_| PlatformError::Storage)?;
        let value:Value=tx.query_one("INSERT INTO gitlab_settings AS s(singleton,origin,host,client_id,client_secret,redirect_uri,enabled,revision,credential_revision) VALUES(true,$1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(singleton) DO UPDATE SET origin=EXCLUDED.origin,host=EXCLUDED.host,client_id=EXCLUDED.client_id,client_secret=EXCLUDED.client_secret,redirect_uri=EXCLUDED.redirect_uri,enabled=EXCLUDED.enabled,revision=EXCLUDED.revision,credential_revision=EXCLUDED.credential_revision RETURNING to_jsonb(s)-'client_secret'-'singleton'",&[&policy.gitlab_origin,&host,&input.client_id,&secret,&input.redirect_uri,&input.enabled,&next,&credential_revision]).map_err(PlatformStore::database_error)?.get(0);
        let revoke = !input.enabled
            || previous.as_ref().is_some_and(|r| {
                r.get::<_, String>(1) != input.client_id
                    || r.get::<_, String>(2) != policy.gitlab_origin
            });
        if revoke {
            tx.execute("UPDATE git_connections SET status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE provider='gitlab' AND status='active'",&[]).map_err(PlatformStore::database_error)?;
            tx.execute("UPDATE project_repositories SET authorization_status='revoked',revision=revision+1,updated_at=clock_timestamp() WHERE connection_id IN (SELECT id FROM git_connections WHERE provider='gitlab') AND authorization_status='authorized'",&[]).map_err(PlatformStore::database_error)?;
        } else {
            tx.execute("UPDATE git_connections SET config_revision=$1 WHERE provider='gitlab' AND status='active'",&[&next]).map_err(PlatformStore::database_error)?;
        }
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "git.configuration_updated",
            Some("gitlab"),
            json!({"revision":next,"enabled":input.enabled}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
    pub fn load(
        &mut self,
        policy: &GitNetworkPolicy,
    ) -> Result<(GitlabConfiguration, Zeroizing<Vec<u8>>), PlatformError> {
        let row=self.store.client.query_opt("SELECT to_jsonb(s)-'client_secret'-'singleton',client_secret,credential_revision FROM gitlab_settings s WHERE singleton AND enabled",&[]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let config: GitlabConfiguration = PlatformStore::decode(row.get(0))?;
        if config.origin != policy.gitlab_origin {
            return Err(PlatformError::Conflict(
                "GitLab operator origin changed; reconfigure application",
            ));
        }
        let encrypted: ProviderSecretEnvelope = PlatformStore::decode(row.get(1))?;
        let secret = self.store.provider_secret_cipher()?.open(
            &Self::context(
                "gitlab-settings",
                &config.host,
                "gitlab_client_secret",
                row.get(2),
            ),
            &encrypted,
        )?;
        Ok((config, secret))
    }
}
