use crate::{
    GitNetworkPolicy, PlatformStore, git_connection_store::GitConnectionStore,
    github_client::GithubClient, github_settings_store::GithubSettingsStore,
    github_token_store::GithubTokenStore, gitlab_client::GitlabClient,
    gitlab_token_store::GitlabTokenStore, remote_repository_decoder::RemoteRepositoryDecoder,
};
use nexora_platform::*;

pub(crate) struct RemoteRepositoryStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> RemoteRepositoryStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    pub fn list(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        connection_id: &str,
        page: u32,
        limit: u16,
        policy: &GitNetworkPolicy,
    ) -> Result<RemoteRepositoryPage, PlatformError> {
        self.list_scope(ctx, Some(project), connection_id, page, limit, policy)
    }
    pub fn list_available(
        &mut self,
        ctx: &AuthContext,
        connection_id: &str,
        page: u32,
        limit: u16,
        policy: &GitNetworkPolicy,
    ) -> Result<RemoteRepositoryPage, PlatformError> {
        self.list_scope(ctx, None, connection_id, page, limit, policy)
    }
    pub fn profile_metadata(
        &mut self,
        ctx: &AuthContext,
        connection_id: &str,
        remote_id: &str,
        full_name: &str,
        policy: &GitNetworkPolicy,
    ) -> Result<serde_json::Value, PlatformError> {
        if remote_id
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .map(|id| id.to_string())
            .as_deref()
            != Some(remote_id)
        {
            return Err(PlatformError::Validation("invalid remote repository id"));
        }
        let connection = self.authority(ctx, None, connection_id)?;
        RepositoryRegistration {
            provider: connection.provider,
            full_name: full_name.into(),
        }
        .locator()?;
        let result = (|| {
            let (value, token) = match connection.provider {
                GitProvider::Github => {
                    let tokens = GithubTokenStore::new(self.store).tokens(&connection, policy)?;
                    let client = GithubClient::new(policy);
                    let (_, scoped) = crate::github_repository_proof::GithubRepositoryProof::check(
                        self.store,
                        &connection,
                        full_name,
                        Some(remote_id),
                        policy,
                    )?;
                    if !scoped.full_name.eq_ignore_ascii_case(full_name) {
                        return Err(PlatformError::NotFound);
                    }
                    (
                        client.get(
                            &format!("/repos/{}", scoped.full_name),
                            &scoped.access_token,
                        )?,
                        zeroize::Zeroizing::new(tokens.access_token.to_string()),
                    )
                }
                GitProvider::Gitlab => {
                    let tokens = GitlabTokenStore::new(self.store).tokens(&connection, policy)?;
                    let client = GitlabClient::new(policy);
                    if client.user(&tokens.access_token)?.0 != connection.remote_account_id {
                        return Err(PlatformError::Unauthorized);
                    }
                    let (actual_id, actual_name) =
                        client.repository(&tokens.access_token, remote_id)?;
                    if actual_id != remote_id || actual_name != full_name {
                        return Err(PlatformError::NotFound);
                    }
                    (
                        client.get(
                            policy.url(&format!("/api/v4/projects/{remote_id}"))?,
                            &tokens.access_token,
                        )?,
                        zeroize::Zeroizing::new(tokens.access_token.to_string()),
                    )
                }
            };
            let repository = crate::repository_profile_metadata::RepositoryProfileMetadata::decode(
                connection.provider,
                remote_id,
                &value,
            )?;
            let query = PullRequestQuery {
                operation: PullRequestOperation::List,
                number: None,
                state: "open".into(),
                page: 1,
                limit: 6,
            };
            let requests = crate::pull_request_store::PullRequestStore::fetch(
                connection.provider,
                policy,
                full_name,
                remote_id,
                &token,
                &query,
            );
            let (requests, available) = match requests {
                Ok(value) => (value["items"].clone(), true),
                Err(
                    PlatformError::Forbidden | PlatformError::NotFound | PlatformError::Unavailable,
                ) => (serde_json::json!([]), false),
                Err(error) => return Err(error),
            };
            let requests: Vec<serde_json::Value> = requests.as_array().into_iter().flatten().map(|value| serde_json::json!({"id":value["number"],"number":value["number"],"title":value["title"],"state":value["state"],"url":value["web_url"]})).collect();
            Ok(
                serde_json::json!({"repository":repository,"pull_requests":requests,"pull_requests_available":available}),
            )
        })();
        if matches!(&result, Err(PlatformError::Unauthorized)) {
            GitConnectionStore::new(self.store).record_access_failure(ctx, connection_id)?;
        }
        let result = result?;
        let current = self.authority(ctx, None, connection_id)?;
        if current.revision != connection.revision {
            return Err(PlatformError::Conflict(
                "connection authority changed during profile read",
            ));
        }
        Ok(result)
    }
    fn authority(
        &mut self,
        ctx: &AuthContext,
        project: Option<&str>,
        connection_id: &str,
    ) -> Result<GitConnection, PlatformError> {
        if let Some(project) = project {
            let connection =
                GitConnectionStore::new(self.store).can_use(ctx, connection_id, project)?;
            self.require_project(project)?;
            Ok(connection)
        } else {
            GitConnectionStore::new(self.store).can_discover(ctx, connection_id)
        }
    }
    fn list_scope(
        &mut self,
        ctx: &AuthContext,
        project: Option<&str>,
        connection_id: &str,
        page: u32,
        limit: u16,
        policy: &GitNetworkPolicy,
    ) -> Result<RemoteRepositoryPage, PlatformError> {
        RemoteRepositoryDecoder::validate(page, limit)?;
        let connection = self.authority(ctx, project, connection_id)?;
        let result = self.fetch(&connection, page, limit, policy);
        if matches!(&result, Err(PlatformError::Unauthorized)) {
            GitConnectionStore::new(self.store).record_access_failure(ctx, connection_id)?;
        }
        let result = result?;
        // Refresh actor, credential and connection authority after remote IO.
        let current = self.authority(ctx, project, connection_id)?;
        if current.revision != connection.revision {
            return Err(PlatformError::Conflict(
                "connection authority changed during discovery",
            ));
        }
        Ok(result)
    }
    fn require_project(&mut self, project: &str) -> Result<(), PlatformError> {
        self.store
            .client
            .query_opt(
                "SELECT id FROM projects WHERE id=$1 AND NOT archived",
                &[&project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        Ok(())
    }
    fn fetch(
        &mut self,
        connection: &GitConnection,
        page: u32,
        limit: u16,
        policy: &GitNetworkPolicy,
    ) -> Result<RemoteRepositoryPage, PlatformError> {
        match connection.provider {
            GitProvider::Github => {
                let tokens = GithubTokenStore::new(self.store).tokens(connection, policy)?;
                let (configuration, secrets) = GithubSettingsStore::new(self.store).load(policy)?;
                let client = GithubClient::new(policy);
                if client.user(&tokens.access_token)?.0 != connection.remote_account_id {
                    return Err(PlatformError::Unauthorized);
                }
                let installation = connection
                    .installation_id
                    .as_deref()
                    .ok_or(PlatformError::Unauthorized)?;
                match client.installation(
                    &configuration,
                    &secrets,
                    installation,
                    &tokens.access_token,
                ) {
                    Err(PlatformError::Forbidden | PlatformError::NotFound) => {
                        return Err(PlatformError::Unauthorized);
                    }
                    result => {
                        result?;
                    }
                }
                client.repositories(&tokens.access_token, installation, page, limit)
            }
            GitProvider::Gitlab => {
                let tokens = GitlabTokenStore::new(self.store).tokens(connection, policy)?;
                let client = GitlabClient::new(policy);
                if client.user(&tokens.access_token)?.0 != connection.remote_account_id {
                    return Err(PlatformError::Unauthorized);
                }
                client.repositories(&tokens.access_token, page, limit)
            }
        }
    }
}
