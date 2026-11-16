use crate::{
    GitNetworkPolicy, PlatformStore, ProviderSecretEnvelope, SecretGenerator,
    audit_writer::AuditWriter, github_client::GithubClient,
    github_settings_store::GithubSettingsStore, github_tokens::GithubTokens,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
use zeroize::Zeroizing;
pub(crate) struct GithubConsentStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> GithubConsentStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn pending(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<(i64, String, String, GithubTokens), PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::ConnectionsManage, None)?;
        let row=self.store.client.query_opt("SELECT a.config_revision,a.credential_id,a.credential_method,c.remote_account_id,c.remote_login,c.tokens FROM github_consents c JOIN git_authorizations a ON a.id=c.id WHERE c.id=$1 AND a.provider='github' AND a.owner_actor_id=$2 AND c.expires_at>now() AND c.token_expires_at>now() AND c.connection_id IS NULL",&[&id,&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        let method = match row.get::<_, String>(2).as_str() {
            "session" => AuthMethod::Session,
            "api_key" => AuthMethod::ApiKey,
            _ => return Err(PlatformError::Storage),
        };
        crate::git_initiator::GitInitiator::load(
            self.store,
            ctx.actor_id(),
            method,
            &row.get::<_, String>(1),
        )?;
        let version: i64 = row.get(0);
        let account: String = row.get(3);
        let login: String = row.get(4);
        let encrypted: ProviderSecretEnvelope = PlatformStore::decode(row.get(5))?;
        let config_row = self
            .store
            .client
            .query_opt(
                "SELECT host FROM github_settings WHERE singleton AND enabled AND revision=$1",
                &[&version],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::Unauthorized)?;
        let host: String = config_row.get(0);
        let plain = self.store.provider_secret_cipher()?.open(
            &GithubSettingsStore::context(id, &host, "github_consent_tokens", 1),
            &encrypted,
        )?;
        let tokens: GithubTokens =
            serde_json::from_slice(&plain).map_err(|_| PlatformError::Storage)?;
        tokens.validate()?;
        Ok((version, account, login, tokens))
    }
    pub fn installations(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        page: u32,
        limit: u16,
        policy: &GitNetworkPolicy,
    ) -> Result<GithubInstallationPage, PlatformError> {
        let (revision, account, _, tokens) = self.pending(ctx, id)?;
        let (config, _) = GithubSettingsStore::new(self.store).load(policy)?;
        if config.revision != revision {
            return Err(PlatformError::Unauthorized);
        }
        let client = GithubClient::new(policy);
        if client.user(&tokens.access_token)?.0 != account {
            return Err(PlatformError::Unauthorized);
        }
        client.installations(&tokens.access_token, &config, page, limit)
    }
    pub fn select(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &GithubInstallationSelection,
        policy: &GitNetworkPolicy,
    ) -> Result<GitConnection, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::ConnectionsManage, None)?;
        let request = self.store.request_context.clone();
        {
            let mut tx = self
                .store
                .client
                .transaction()
                .map_err(PlatformStore::database_error)?;
            let receipt = ReceiptWriter::read(
                &mut tx,
                &self.store.namespace,
                request.as_ref(),
                ctx.actor_id(),
                "github.connection.select",
            )?;
            tx.commit().map_err(PlatformStore::database_error)?;
            if let Some(v) = receipt {
                return PlatformStore::decode(v);
            }
        }
        let (revision, account, login, tokens) = self.pending(&ctx, id)?;
        let (config, secrets) = GithubSettingsStore::new(self.store).load(policy)?;
        if config.revision != revision {
            return Err(PlatformError::Unauthorized);
        }
        let client = GithubClient::new(policy);
        if client.user(&tokens.access_token)?.0 != account {
            return Err(PlatformError::Unauthorized);
        }
        let installation = client.installation(
            &config,
            &secrets,
            &input.installation_id,
            &tokens.access_token,
        )?;
        let ctx = self.store.fresh(&ctx)?;
        ctx.require(Capability::ConnectionsManage, None)?;
        let connection = SecretGenerator::id("gcn")?;
        let cipher = self.store.provider_secret_cipher()?;
        let plain =
            Zeroizing::new(serde_json::to_vec(&tokens).map_err(|_| PlatformError::Storage)?);
        let encrypted = serde_json::to_value(cipher.seal(
            &GithubSettingsStore::context(&connection, &config.host, "github_user_tokens", 1),
            &plain,
        )?)
        .map_err(|_| PlatformError::Storage)?;
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "github.connection.select",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(v);
        }
        tx.query_opt("SELECT revision FROM github_settings WHERE singleton AND enabled AND revision=$1 FOR SHARE",&[&revision]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let row=tx.query_opt("SELECT c.token_expires_at FROM github_consents c JOIN git_authorizations a ON a.id=c.id WHERE c.id=$1 AND a.owner_actor_id=$2 AND c.expires_at>now() AND c.token_expires_at>now() AND c.connection_id IS NULL FOR UPDATE OF c",&[&id,&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Conflict("GitHub consent already used or expired"))?;
        let _ = row;
        let value:Value=tx.query_one("INSERT INTO git_connections AS c(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at,installation_id) SELECT $1,'github',$2,$3,$4,$5,'active',$6,$7,token_expires_at,$8 FROM github_consents WHERE id=$9 RETURNING to_jsonb(c)-'tokens'",&[&connection,&config.host,&ctx.actor_id(),&account,&login,&revision,&encrypted,&installation.id,&id]).map_err(PlatformStore::database_error)?.get(0);
        tx.execute(
            "UPDATE github_consents SET connection_id=$2 WHERE id=$1",
            &[&id, &connection],
        )
        .map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "git.connection_authorized",
            Some(&connection),
            json!({"provider":"github","remote_account_id":account,"installation_id":installation.id}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
}
