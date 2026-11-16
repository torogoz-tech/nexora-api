use crate::{
    GitNetworkPolicy, PlatformStore, remote_repository_store::RemoteRepositoryStore,
    repository_profile_store::RepositoryProfileStore,
};
use nexora_platform::{AuthContext, PlatformError};
impl PlatformStore {
    pub fn repository_profile_with_provider(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        year: Option<i32>,
        policy: &GitNetworkPolicy,
    ) -> Result<serde_json::Value, PlatformError> {
        let baseline = RepositoryProfileStore::registered(self, ctx, project, repository, year)?;
        if baseline["repository"]["provider"].is_null() {
            return Ok(baseline);
        }
        let query = nexora_platform::PullRequestQuery {
            operation: nexora_platform::PullRequestOperation::List,
            number: None,
            state: "open".into(),
            page: 1,
            limit: 6,
        };
        let requests = crate::pull_request_store::PullRequestStore::new(self)
            .query(ctx, project, repository, &query, policy);
        // Reload live project, credential and source authority after provider IO.
        let mut result = RepositoryProfileStore::registered(self, ctx, project, repository, year)?;
        match requests {
            Ok(requests) => {
                let items: Vec<serde_json::Value> = requests["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|value| serde_json::json!({"id":value["number"],"number":value["number"],"title":value["title"],"state":value["state"],"url":value["web_url"]}))
                    .collect();
                result["counts"]["pull_requests"] = serde_json::json!(items.len());
                result["pull_requests"] = serde_json::json!(items);
                result["pull_requests_available"] = serde_json::json!(true);
            }
            Err(
                PlatformError::Forbidden | PlatformError::NotFound | PlatformError::Unavailable,
            ) => {}
            Err(error) => return Err(error),
        }
        Ok(result)
    }
    pub fn repository_profile(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        year: Option<i32>,
    ) -> Result<serde_json::Value, PlatformError> {
        RepositoryProfileStore::registered(self, ctx, project, repository, year)
    }
    pub fn remote_repository_profile(
        &mut self,
        ctx: &AuthContext,
        connection: &str,
        remote: &str,
        full_name: &str,
        year: Option<i32>,
        policy: &GitNetworkPolicy,
    ) -> Result<serde_json::Value, PlatformError> {
        let metadata = RemoteRepositoryStore::new(self)
            .profile_metadata(ctx, connection, remote, full_name, policy)?;
        RepositoryProfileStore::remote(self, ctx, connection, remote, metadata, year)
    }
}
