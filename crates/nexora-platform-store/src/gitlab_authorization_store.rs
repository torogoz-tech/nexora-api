use crate::{
    GitNetworkPolicy, PlatformStore, ProviderSecretEnvelope, SecretGenerator,
    audit_writer::AuditWriter, gitlab_client::GitlabClient,
    gitlab_settings_store::GitlabSettingsStore,
};
use nexora_platform::*;
use serde_json::{Value, json};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;
pub(crate) struct GitlabAuthorizationStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> GitlabAuthorizationStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn start(
        &mut self,
        ctx: &AuthContext,
        policy: &GitNetworkPolicy,
    ) -> Result<GitAuthorization, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::ConnectionsManage, None)?;
        let (config, _) = GitlabSettingsStore::new(self.store).load(policy)?;
        let cipher = self.store.provider_secret_cipher()?;
        let id = SecretGenerator::id("gsa")?;
        let state = format!("{id}.{}", SecretGenerator::token("state")?);
        let correlation = SecretGenerator::token("browser")?;
        let verifier = Zeroizing::new(SecretGenerator::token("pkce")?);
        let url = GitlabClient::new(policy).authorization_url(&config, &state, &verifier)?;
        let encrypted = serde_json::to_value(cipher.seal(
            &GitlabSettingsStore::context(&id, &config.host, "gitlab_pkce", 1),
            verifier.as_bytes(),
        )?)
        .map_err(|_| PlatformError::Storage)?;
        let method = match ctx.method() {
            AuthMethod::Session => "session",
            AuthMethod::ApiKey => "api_key",
        };
        let n=self.store.client.execute("INSERT INTO git_authorizations(id,provider,config_revision,owner_actor_id,credential_id,credential_method,state_digest,cookie_digest,verifier,expires_at) SELECT $1,'gitlab',revision,$2,$3,$4,$5,$6,$7,now()+interval '10 minutes' FROM gitlab_settings WHERE singleton AND enabled AND revision=$8",&[&id,&ctx.actor_id(),&ctx.credential_id(),&method,&SecretGenerator::digest(&state),&SecretGenerator::digest(&correlation),&encrypted,&config.revision]).map_err(PlatformStore::database_error)?;
        if n != 1 {
            return Err(PlatformError::Unauthorized);
        }
        Ok(GitAuthorization {
            authorization_url: url,
            state_id: id,
            correlation_secret: correlation,
            secure_cookie: config.redirect_uri.starts_with("https://"),
        })
    }
    pub fn complete(
        &mut self,
        state: &str,
        code: &str,
        cookie: &str,
        policy: &GitNetworkPolicy,
    ) -> Result<GitConnection, PlatformError> {
        if state.len() > 256 || cookie.len() > 256 || code.len() > 4096 {
            return Err(PlatformError::Unauthorized);
        }
        let (id, _) = state.split_once('.').ok_or(PlatformError::Unauthorized)?;
        if id.len() != 36
            || !id.starts_with("gsa_")
            || !id[4..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(PlatformError::Unauthorized);
        }
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let row=tx.query_opt("SELECT owner_actor_id,credential_method,credential_id,config_revision,state_digest,cookie_digest,verifier,expires_at>now(),consumed_at IS NULL FROM git_authorizations WHERE id=$1 AND provider='gitlab' FOR UPDATE",&[&id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let sh: Vec<u8> = row.get(4);
        let ch: Vec<u8> = row.get(5);
        if !row.get::<_, bool>(7)
            || !row.get::<_, bool>(8)
            || !bool::from(sh.ct_eq(&SecretGenerator::digest(state)))
            || !bool::from(ch.ct_eq(&SecretGenerator::digest(cookie)))
        {
            return Err(PlatformError::Unauthorized);
        }
        let actor: String = row.get(0);
        let method = match row.get::<_, String>(1).as_str() {
            "session" => AuthMethod::Session,
            "api_key" => AuthMethod::ApiKey,
            _ => return Err(PlatformError::Storage),
        };
        let credential: String = row.get(2);
        let revision: i64 = row.get(3);
        let verifier: ProviderSecretEnvelope = PlatformStore::decode(row.get(6))?;
        tx.execute(
            "UPDATE git_authorizations SET consumed_at=now() WHERE id=$1",
            &[&id],
        )
        .map_err(PlatformStore::database_error)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        if code.is_empty() {
            return Err(PlatformError::Unauthorized);
        }
        crate::git_initiator::GitInitiator::load(self.store, &actor, method, &credential)?;
        let (config, client_secret) = GitlabSettingsStore::new(self.store).load(policy)?;
        if config.revision != revision {
            return Err(PlatformError::Unauthorized);
        }
        let cipher = self.store.provider_secret_cipher()?;
        let verifier = cipher.open(
            &GitlabSettingsStore::context(id, &config.host, "gitlab_pkce", 1),
            &verifier,
        )?;
        let client = GitlabClient::new(policy);
        let tokens = client.exchange(
            &config,
            std::str::from_utf8(&client_secret).map_err(|_| PlatformError::Storage)?,
            code,
            std::str::from_utf8(&verifier).map_err(|_| PlatformError::Storage)?,
        )?;
        let (account, login) = client.user(&tokens.access_token)?;
        let ctx =
            crate::git_initiator::GitInitiator::load(self.store, &actor, method, &credential)?;
        let connection = SecretGenerator::id("gcn")?;
        let plain =
            Zeroizing::new(serde_json::to_vec(&tokens).map_err(|_| PlatformError::Storage)?);
        let encrypted = serde_json::to_value(cipher.seal(
            &GitlabSettingsStore::context(&connection, &config.host, "gitlab_tokens", 1),
            &plain,
        )?)
        .map_err(|_| PlatformError::Storage)?;
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        tx.query_opt("SELECT revision FROM gitlab_settings WHERE singleton AND enabled AND revision=$1 FOR SHARE",&[&revision]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let value:Value=tx.query_one("INSERT INTO git_connections AS c(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab',$2,$3,$4,$5,'active',$6,$7,now()+$8::bigint*interval '1 second') RETURNING to_jsonb(c)-'tokens'",&[&connection,&config.host,&ctx.actor_id(),&account,&login,&revision,&encrypted,&(tokens.expires_in as i64)]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "git.connection_authorized",
            Some(&connection),
            json!({"provider":"gitlab","remote_account_id":account}),
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        PlatformStore::decode(value)
    }
}
