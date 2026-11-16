use crate::{
    GitNetworkPolicy, PlatformStore, git_commit_identity::GitCommitIdentity,
    github_client::GithubClient, github_repository_proof::GithubRepositoryProof,
    pull_request_source::PullRequestSource,
};
use nexora_platform::{AuthContext, GitProvider, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn repository_commit_detail(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        sha: &str,
        page: u16,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        if !GitCommitIdentity::valid(sha) || page == 0 || page > 150 {
            return Err(PlatformError::Validation("invalid commit page"));
        }
        let source = PullRequestSource::load(self, ctx, project, repository, policy)?;
        if source.connection.provider != GitProvider::Github {
            return Err(PlatformError::Validation("commit details require GitHub"));
        }
        let name = source
            .repository
            .remote_full_name
            .as_deref()
            .ok_or(PlatformError::NotFound)?;
        let remote = source
            .repository
            .remote_id
            .as_deref()
            .ok_or(PlatformError::NotFound)?;
        let (_, token) =
            GithubRepositoryProof::check(self, &source.connection, name, Some(remote), policy)?;
        let value = GithubClient::new(policy).get(
            &format!(
                "/repos/{}/commits/{sha}?per_page=20&page={page}",
                token.full_name
            ),
            &token.access_token,
        )?;
        if value["sha"].as_str() != Some(sha) {
            return Err(PlatformError::Unavailable);
        }
        let files = value["files"]
            .as_array()
            .filter(|files| files.len() <= 20)
            .ok_or(PlatformError::Unavailable)?;
        let entries:Vec<Value>=files.iter().map(|file|json!({"path":file["filename"],"previous_path":file["previous_filename"],"status":file["status"],"additions":file["additions"],"deletions":file["deletions"],"patch":file["patch"]})).collect();
        let current = PullRequestSource::load(self, ctx, project, repository, policy)?;
        if !source.matches(&current) {
            return Err(PlatformError::Conflict("repository source changed"));
        }
        Ok(
            json!({"sha":sha,"message":value["commit"]["message"],"author":value["commit"]["author"]["name"],"created_at":value["commit"]["committer"]["date"],"files":entries,"page":page,"has_more":files.len()==20&&page<150}),
        )
    }
}
