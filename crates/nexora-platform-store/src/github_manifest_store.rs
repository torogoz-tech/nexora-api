use crate::{
    GitNetworkPolicy, PlatformStore, SecretGenerator, authentication_store::AuthenticationStore,
    github_client::GithubClient, github_settings_store::GithubSettingsStore,
};
use nexora_platform::*;
use serde_json::json;
use subtle::ConstantTimeEq;
pub(crate) struct GithubManifestStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> GithubManifestStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn start(
        &mut self,
        ctx: &AuthContext,
        input: &GithubManifestRequest,
        policy: &GitNetworkPolicy,
    ) -> Result<GithubManifestAuthorization, PlatformError> {
        let ctx = self.store.fresh(ctx)?;
        ctx.require(Capability::InstallationManage, None)?;
        policy.validate()?;
        if input.name.trim().is_empty()
            || input.name.len() > 100
            || input.name.chars().any(char::is_control)
            || input.expected_revision < 0
            || input.organization.as_ref().is_some_and(|v| {
                v.is_empty()
                    || v.len() > 39
                    || !v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return Err(PlatformError::Validation("invalid GitHub manifest request"));
        }
        let revision: i64 = self
            .store
            .client
            .query_opt("SELECT revision FROM github_settings WHERE singleton", &[])
            .map_err(PlatformStore::database_error)?
            .map_or(0, |r| r.get(0));
        if revision != input.expected_revision {
            return Err(PlatformError::Conflict(
                "GitHub configuration revision changed",
            ));
        }
        let id = SecretGenerator::id("gms")?;
        let state = format!("{id}.{}", SecretGenerator::token("state")?);
        let cookie = SecretGenerator::token("browser")?;
        let method = match ctx.method() {
            AuthMethod::Session => "session",
            AuthMethod::ApiKey => "api_key",
        };
        self.store.client.execute("INSERT INTO github_manifest_authorizations(id,owner_actor_id,credential_id,credential_method,state_digest,cookie_digest,expected_revision,origin,api_origin,callback_origin,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,now()+interval '10 minutes')",&[&id,&ctx.actor_id(),&ctx.credential_id(),&method,&SecretGenerator::digest(&state),&SecretGenerator::digest(&cookie),&input.expected_revision,&policy.github_origin,&policy.github_api_origin,&policy.callback_origin]).map_err(PlatformStore::database_error)?;
        let target = match &input.organization {
            Some(org) => format!("/organizations/{org}/settings/apps/new"),
            None => "/settings/apps/new".into(),
        };
        let mut action = policy.github_url(&target, false)?;
        action.query_pairs_mut().append_pair("state", &state);
        let callback = format!("{}/api/v1/git/github/callback", policy.callback_origin);
        let manifest = json!({"name":input.name.trim(),"url":policy.callback_origin,"redirect_url":format!("{}/api/v1/git/github/manifest/callback",policy.callback_origin),"callback_urls":[callback],"hook_attributes":{"url":format!("{}/api/v1/git/github/webhook",policy.callback_origin),"active":true},"default_permissions":{"contents":"write","metadata":"read","pull_requests":"write","actions":"read","checks":"read","deployments":"read","administration":"read"},"default_events":["push","repository"],"public":input.public,"request_oauth_on_install":true});
        Ok(GithubManifestAuthorization {
            action_url: action.into(),
            manifest,
            state_id: id,
            correlation_secret: cookie,
            secure_cookie: policy.callback_origin.starts_with("https://"),
        })
    }
    pub fn complete(
        &mut self,
        state: &str,
        code: &str,
        cookie: &str,
        policy: &GitNetworkPolicy,
    ) -> Result<GithubConfiguration, PlatformError> {
        if state.len() > 256 || code.len() > 512 || cookie.len() > 256 {
            return Err(PlatformError::Unauthorized);
        }
        let (id, _) = state.split_once('.').ok_or(PlatformError::Unauthorized)?;
        if id.len() != 36
            || !id.starts_with("gms_")
            || !id[4..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(PlatformError::Unauthorized);
        }
        let mut tx = self
            .store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let row=tx.query_opt("SELECT owner_actor_id,credential_id,credential_method,state_digest,cookie_digest,expected_revision,origin,api_origin,callback_origin FROM github_manifest_authorizations WHERE id=$1 AND expires_at>now() AND consumed_at IS NULL FOR UPDATE",&[&id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let state_digest: Vec<u8> = row.get(3);
        let cookie_digest: Vec<u8> = row.get(4);
        if !bool::from(state_digest.ct_eq(&SecretGenerator::digest(state)))
            || !bool::from(cookie_digest.ct_eq(&SecretGenerator::digest(cookie)))
            || row.get::<_, String>(6) != policy.github_origin
            || row.get::<_, String>(7) != policy.github_api_origin
            || row.get::<_, String>(8) != policy.callback_origin
        {
            return Err(PlatformError::Unauthorized);
        }
        let actor: String = row.get(0);
        let credential: String = row.get(1);
        let method = match row.get::<_, String>(2).as_str() {
            "session" => AuthMethod::Session,
            "api_key" => AuthMethod::ApiKey,
            _ => return Err(PlatformError::Storage),
        };
        let revision: i64 = row.get(5);
        tx.execute(
            "UPDATE github_manifest_authorizations SET consumed_at=now() WHERE id=$1",
            &[&id],
        )
        .map_err(PlatformStore::database_error)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        let ctx = AuthenticationStore::new(self.store).load(method, &credential)?;
        if ctx.actor_id() != actor {
            return Err(PlatformError::Unauthorized);
        }
        ctx.require(Capability::InstallationManage, None)?;
        let registration = GithubClient::new(policy).convert_manifest(code, revision)?;
        let input = serde_json::to_value(&registration).map_err(|_| PlatformError::Storage)?;
        self.store
            .request(&ctx, "github.configure", id, &input, |s| {
                GithubSettingsStore::new(s).configure(&ctx, &registration, policy)
            })
    }
}
