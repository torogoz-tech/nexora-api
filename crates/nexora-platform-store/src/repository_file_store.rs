use crate::{
    GitNetworkPolicy, PlatformStore, github_client::GithubClient,
    github_repository_proof::GithubRepositoryProof, gitlab_client::GitlabClient,
    gitlab_token_store::GitlabTokenStore, pull_request_source::PullRequestSource,
    repository_binding_store::RepositoryBindingStore,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use nexora_platform::{AuthContext, GitProvider, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn repository_file(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        path: &str,
        commit: &str,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        if path.is_empty()
            || path.len() > 4096
            || path.contains('\\')
            || path.chars().any(char::is_control)
            || path
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
        {
            return Err(PlatformError::Validation("invalid source path"));
        }
        let binding = RepositoryBindingStore::get(self, ctx, project, repository)?;
        if commit != binding.pinned_commit {
            return Err(PlatformError::Conflict("repository snapshot changed"));
        }
        let source = PullRequestSource::load(self, ctx, project, repository, policy)?;
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
        let encode =
            |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
        let value = match source.connection.provider {
            GitProvider::Github => {
                let (_, scoped) = GithubRepositoryProof::check(
                    self,
                    &source.connection,
                    name,
                    Some(remote),
                    policy,
                )?;
                let encoded = path.split('/').map(encode).collect::<Vec<_>>().join("/");
                GithubClient::new(policy).get(
                    &format!(
                        "/repos/{}/contents/{}?ref={}",
                        scoped.full_name,
                        encoded,
                        encode(commit)
                    ),
                    &scoped.access_token,
                )?
            }
            GitProvider::Gitlab => {
                let tokens = GitlabTokenStore::new(self).tokens(&source.connection, policy)?;
                let client = GitlabClient::new(policy);
                if client.user(&tokens.access_token)?.0 != source.connection.remote_account_id
                    || client.repository(&tokens.access_token, remote)?.0 != remote
                {
                    return Err(PlatformError::Forbidden);
                }
                client.get(
                    policy.url(&format!(
                        "/api/v4/projects/{remote}/repository/files/{}?ref={}",
                        encode(path),
                        encode(commit)
                    ))?,
                    &tokens.access_token,
                )?
            }
        };
        if value["encoding"] != "base64" || value["size"].as_u64().is_none_or(|s| s > 524288) {
            return Err(PlatformError::Validation(
                "source file too large or not text",
            ));
        }
        let encoded = value["content"]
            .as_str()
            .ok_or(PlatformError::Unavailable)?
            .replace(['\n', '\r'], "");
        if encoded.len() > 710000 {
            return Err(PlatformError::Validation("source file too large"));
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| PlatformError::Unavailable)?;
        let current = PullRequestSource::load(self, ctx, project, repository, policy)?;
        let current_binding = RepositoryBindingStore::get(self, ctx, project, repository)?;
        if !source.matches(&current) || binding.pinned_commit != current_binding.pinned_commit {
            return Err(PlatformError::Conflict("repository source changed"));
        }
        let (content, encoding, warning) = match nexora_indexer::SourceTextDecoder::decode(&bytes) {
            nexora_indexer::SourceReadOutcome::Decoded {
                content, encoding, ..
            } if !content.contains('\0') => (Some(content), Some(encoding), None),
            nexora_indexer::SourceReadOutcome::Decoded { .. } => (
                None,
                None,
                Some("binary content cannot be displayed as source".into()),
            ),
            nexora_indexer::SourceReadOutcome::Skipped { reason, .. } => (None, None, Some(reason)),
        };
        Ok(
            json!({"path":path,"commit":commit,"content":content,"encoding":encoding,"warning":warning}),
        )
    }
}
