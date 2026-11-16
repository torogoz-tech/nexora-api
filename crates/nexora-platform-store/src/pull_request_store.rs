use crate::{
    GitNetworkPolicy, PlatformStore, github_repository_proof::GithubRepositoryProof,
    github_token_store::GithubTokenStore, gitlab_client::GitlabClient,
    gitlab_token_store::GitlabTokenStore, pull_request_client::PullRequestClient,
    pull_request_decoder::PullRequestDecoder, pull_request_source::PullRequestSource,
};
use nexora_platform::*;
use serde_json::{Value, json};
use zeroize::Zeroizing;
pub struct PullRequestStore<'a> {
    store: &'a mut PlatformStore,
}
impl<'a> PullRequestStore<'a> {
    pub fn new(store: &'a mut PlatformStore) -> Self {
        Self { store }
    }
    fn provider_error(error: PlatformError) -> PlatformError {
        match error {
            PlatformError::Unauthorized => {
                PlatformError::Conflict("Git provider authorization requires reconnection")
            }
            other => other,
        }
    }
    pub fn query(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        query: &PullRequestQuery,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        query.validate()?;
        let source = PullRequestSource::load(self.store, ctx, project, repository, policy)?;
        let id = source
            .repository
            .remote_id
            .as_deref()
            .ok_or(PlatformError::Storage)?;
        let name = source
            .repository
            .remote_full_name
            .as_deref()
            .ok_or(PlatformError::Storage)?;
        let token: Zeroizing<String> = match source.connection.provider {
            GitProvider::Github => {
                GithubRepositoryProof::check(
                    self.store,
                    &source.connection,
                    name,
                    Some(id),
                    policy,
                )
                .map_err(Self::provider_error)?;
                let tokens = GithubTokenStore::new(self.store)
                    .tokens(&source.connection, policy)
                    .map_err(Self::provider_error)?;
                Zeroizing::new(tokens.access_token.to_string())
            }
            GitProvider::Gitlab => {
                let tokens = GitlabTokenStore::new(self.store)
                    .tokens(&source.connection, policy)
                    .map_err(Self::provider_error)?;
                let client = GitlabClient::new(policy);
                if client
                    .user(&tokens.access_token)
                    .map_err(Self::provider_error)?
                    .0
                    != source.connection.remote_account_id
                    || client
                        .repository(&tokens.access_token, id)
                        .map_err(Self::provider_error)?
                        .0
                        != id
                {
                    return Err(PlatformError::Conflict(
                        "Git provider repository identity changed",
                    ));
                }
                Zeroizing::new(tokens.access_token.to_string())
            }
        };
        let baseline = PullRequestSource::load(self.store, ctx, project, repository, policy)?;
        if source.repository.revision != baseline.repository.revision
            || source.connection.revision != baseline.connection.revision
        {
            return Err(PlatformError::Conflict(
                "repository authority changed during provider verification",
            ));
        }
        let result = Self::fetch(source.connection.provider, policy, name, id, &token, query)?;
        let current = PullRequestSource::load(self.store, ctx, project, repository, policy)?;
        if !baseline.matches(&current) {
            return Err(PlatformError::Conflict(
                "repository authority changed during provider request",
            ));
        }
        Ok(result)
    }
    fn array(value: Value) -> Result<Vec<Value>, PlatformError> {
        value.as_array().cloned().ok_or(PlatformError::Unavailable)
    }
    pub(crate) fn fetch(
        provider: GitProvider,
        policy: &GitNetworkPolicy,
        name: &str,
        id: &str,
        token: &str,
        q: &PullRequestQuery,
    ) -> Result<Value, PlatformError> {
        let github = provider == GitProvider::Github;
        RepositoryRegistration {
            provider,
            full_name: name.into(),
        }
        .locator()?;
        if !github && id.parse::<u64>().ok().filter(|v| *v > 0).is_none() {
            return Err(PlatformError::Unavailable);
        }
        let base = if github {
            format!("/repos/{name}/pulls")
        } else {
            format!("/api/v4/projects/{id}/merge_requests")
        };
        let client = PullRequestClient::new(policy, provider);
        let paging = format!("page={}&per_page={}", q.page, q.limit);
        let number = q.number.unwrap_or(0);
        let detail = format!("{base}/{number}");
        if q.operation == PullRequestOperation::Detail {
            return PullRequestDecoder::summary(provider, &client.get(&detail, token)?);
        }
        let (raw, extra) = match q.operation {
            PullRequestOperation::List => {
                let state = if !github && q.state == "open" {
                    "opened"
                } else {
                    q.state.as_str()
                };
                (
                    Self::array(client.get(&format!("{base}?state={state}&{paging}"), token)?)?,
                    Vec::new(),
                )
            }
            PullRequestOperation::Files => (
                Self::array(client.get(
                    &format!(
                        "{detail}/{}?{paging}",
                        if github { "files" } else { "diffs" }
                    ),
                    token,
                )?)?,
                Vec::new(),
            ),
            PullRequestOperation::Comments => {
                if github {
                    (
                        Self::array(client.get(
                            &format!("/repos/{name}/issues/{number}/comments?{paging}"),
                            token,
                        )?)?,
                        Self::array(client.get(&format!("{detail}/comments?{paging}"), token)?)?,
                    )
                } else {
                    (
                        Self::array(client.get(&format!("{detail}/notes?{paging}"), token)?)?,
                        Vec::new(),
                    )
                }
            }
            PullRequestOperation::Checks => {
                if github {
                    let current = client.get(&detail, token)?;
                    let sha = current["head"]["sha"]
                        .as_str()
                        .filter(|s| s.len() == 40 || s.len() == 64)
                        .filter(|s| s.bytes().all(|b| b.is_ascii_hexdigit()))
                        .ok_or(PlatformError::Unavailable)?;
                    let checks = client.get(
                        &format!("/repos/{name}/commits/{sha}/check-runs?{paging}"),
                        token,
                    )?;
                    (
                        Self::array(checks["check_runs"].clone())?,
                        Self::array(client.get(
                            &format!("/repos/{name}/commits/{sha}/statuses?{paging}"),
                            token,
                        )?)?,
                    )
                } else {
                    (
                        Self::array(client.get(&format!("{detail}/pipelines?{paging}"), token)?)?,
                        Vec::new(),
                    )
                }
            }
            _ => return Err(PlatformError::Validation("invalid pull request operation")),
        };
        if raw.len() > usize::from(q.limit) || extra.len() > usize::from(q.limit) {
            return Err(PlatformError::Unavailable);
        }
        let next = (raw.len() == usize::from(q.limit) || extra.len() == usize::from(q.limit))
            .then_some(q.page + 1)
            .filter(|page| *page <= 10000);
        let mut items = Vec::new();
        for value in &raw {
            items.push(match q.operation {
                PullRequestOperation::List => PullRequestDecoder::summary(provider, value)?,
                PullRequestOperation::Files => PullRequestDecoder::file(provider, value)?,
                PullRequestOperation::Comments => {
                    PullRequestDecoder::comment(provider, value, false)?
                }
                PullRequestOperation::Checks => PullRequestDecoder::check(value, provider),
                _ => return Err(PlatformError::Storage),
            });
        }
        for value in &extra {
            items.push(if q.operation == PullRequestOperation::Comments {
                PullRequestDecoder::comment(provider, value, true)?
            } else {
                PullRequestDecoder::check(value, provider)
            });
        }
        let result = json!({"items":items,"next_page":next,"page":q.page});
        if serde_json::to_vec(&result)
            .map_err(|_| PlatformError::Storage)?
            .len()
            > 1_048_576
        {
            return Err(PlatformError::Validation(
                "pull request response exceeds budget; reduce limit",
            ));
        }
        Ok(result)
    }
}
