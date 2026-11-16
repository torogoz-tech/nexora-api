use crate::{GitNetworkPolicy, gitlab_tokens::GitlabTokens};
use nexora_platform::{GitProvider, GitlabConfiguration, PlatformError, RepositoryRegistration};
use openidconnect::{CsrfToken, PkceCodeChallenge, PkceCodeVerifier};
use serde_json::Value;
pub(crate) struct GitlabClient {
    agent: ureq::Agent,
    policy: GitNetworkPolicy,
}
impl GitlabClient {
    pub fn commit(
        &self,
        token: &str,
        id: &str,
        pinned: Option<&str>,
    ) -> Result<String, PlatformError> {
        if pinned.is_some_and(|oid| !crate::git_commit_identity::GitCommitIdentity::valid(oid)) {
            return Err(PlatformError::Conflict("invalid pinned commit"));
        }
        let path = format!("/api/v4/projects/{id}/repository/commits");
        let mut uri = self.policy.url(&path)?;
        if let Some(oid) = pinned {
            uri.path_segments_mut()
                .map_err(|_| PlatformError::Validation("invalid Git endpoint"))?
                .push(oid);
        } else {
            uri.query_pairs_mut().append_pair("per_page", "1");
        }
        let value = self.get(uri, token)?;
        crate::git_commit_identity::GitCommitIdentity::response(&value, "id", pinned)
    }
    pub fn new(policy: &GitNetworkPolicy) -> Self {
        Self {
            agent: ureq::Agent::config_builder()
                .proxy(None)
                .max_redirects(0)
                .http_status_as_error(false)
                .timeout_global(Some(std::time::Duration::from_secs(10)))
                .build()
                .into(),
            policy: policy.clone(),
        }
    }
    pub fn authorization_url(
        &self,
        config: &GitlabConfiguration,
        state: &str,
        verifier: &str,
    ) -> Result<String, PlatformError> {
        let mut uri = self.policy.url("/oauth/authorize")?;
        let pkce =
            PkceCodeChallenge::from_code_verifier_sha256(&PkceCodeVerifier::new(verifier.into()));
        uri.query_pairs_mut().extend_pairs([
            ("client_id", config.client_id.as_str()),
            ("redirect_uri", config.redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", "api"),
            ("state", CsrfToken::new(state.into()).secret()),
            ("code_challenge", pkce.as_str()),
            ("code_challenge_method", "S256"),
        ]);
        Ok(uri.into())
    }
    pub fn exchange(
        &self,
        config: &GitlabConfiguration,
        secret: &str,
        code: &str,
        verifier: &str,
    ) -> Result<GitlabTokens, PlatformError> {
        self.token_request(&[
            ("client_id", config.client_id.as_str()),
            ("client_secret", secret),
            ("redirect_uri", config.redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", verifier),
        ])
    }
    pub fn refresh(
        &self,
        config: &GitlabConfiguration,
        secret: &str,
        refresh: &str,
    ) -> Result<GitlabTokens, PlatformError> {
        self.token_request(&[
            ("client_id", config.client_id.as_str()),
            ("client_secret", secret),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
        ])
    }
    fn token_request(&self, form: &[(&str, &str)]) -> Result<GitlabTokens, PlatformError> {
        let mut response = self
            .agent
            .post(self.policy.url("/oauth/token")?.as_str())
            .header("Accept", "application/json")
            .send_form(form.iter().copied())
            .map_err(|_| PlatformError::Unavailable)?;
        if !response.status().is_success() {
            return Err(if response.status().as_u16() >= 500 {
                PlatformError::Unavailable
            } else {
                PlatformError::Unauthorized
            });
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(1_048_576)
            .read_to_vec()
            .map_err(|_| PlatformError::Unavailable)?;
        let tokens: GitlabTokens =
            serde_json::from_slice(&bytes).map_err(|_| PlatformError::Unauthorized)?;
        tokens.validate()?;
        Ok(tokens)
    }
    pub(crate) fn get(&self, uri: url::Url, token: &str) -> Result<Value, PlatformError> {
        let mut response = self
            .agent
            .get(uri.as_str())
            .header("Authorization", format!("Bearer {token}"))
            .call()
            .map_err(|_| PlatformError::Unavailable)?;
        match response.status().as_u16() {
            200 => {}
            401 => return Err(PlatformError::Unauthorized),
            403 => return Err(PlatformError::Forbidden),
            404 => return Err(PlatformError::NotFound),
            _ => return Err(PlatformError::Unavailable),
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(1_048_576)
            .read_to_vec()
            .map_err(|_| PlatformError::Unavailable)?;
        serde_json::from_slice(&bytes).map_err(|_| PlatformError::Unavailable)
    }
    pub fn user(&self, token: &str) -> Result<(String, String), PlatformError> {
        let value = self.get(self.policy.url("/api/v4/user")?, token)?;
        let id = value["id"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or(PlatformError::Unauthorized)?;
        let username = value["username"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 160)
            .ok_or(PlatformError::Unauthorized)?;
        if value["state"].as_str().is_some_and(|s| s != "active") {
            return Err(PlatformError::Unauthorized);
        }
        Ok((id.to_string(), username.into()))
    }
    pub fn repositories(
        &self,
        token: &str,
        page: u32,
        limit: u16,
    ) -> Result<nexora_platform::RemoteRepositoryPage, PlatformError> {
        crate::remote_repository_decoder::RemoteRepositoryDecoder::validate(page, limit)?;
        let mut uri = self.policy.url("/api/v4/projects")?;
        uri.query_pairs_mut().extend_pairs([
            ("membership", "true"),
            ("simple", "true"),
            ("order_by", "id"),
            ("sort", "asc"),
            ("page", page.to_string().as_str()),
            ("per_page", limit.to_string().as_str()),
        ]);
        let value = self.get(uri, token)?;
        crate::remote_repository_decoder::RemoteRepositoryDecoder::decode(
            GitProvider::Gitlab,
            &value,
            page,
            limit,
        )
    }
    pub fn repository(
        &self,
        token: &str,
        locator: &str,
    ) -> Result<(String, String), PlatformError> {
        let mut uri = self.policy.url("/api/v4/projects/")?;
        uri.path_segments_mut()
            .map_err(|_| PlatformError::Validation("invalid Git endpoint"))?
            .pop_if_empty()
            .push(locator);
        let value = self.get(uri, token)?;
        let id = value["id"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or(PlatformError::Unavailable)?
            .to_string();
        let name = value["path_with_namespace"]
            .as_str()
            .ok_or(PlatformError::Unavailable)?;
        RepositoryRegistration {
            provider: GitProvider::Gitlab,
            full_name: name.into(),
        }
        .locator()?;
        if value["repository_access_level"] == "disabled" {
            return Err(PlatformError::Forbidden);
        }
        let mut branches = self
            .policy
            .url(&format!("/api/v4/projects/{id}/repository/branches"))?;
        branches.query_pairs_mut().append_pair("per_page", "1");
        if !self.get(branches, token)?.is_array() {
            return Err(PlatformError::Unavailable);
        }
        Ok((id, name.into()))
    }
}
