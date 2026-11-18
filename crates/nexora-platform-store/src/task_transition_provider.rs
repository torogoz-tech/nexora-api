use crate::{
    GitNetworkPolicy, PlatformStore, pull_request_client::PullRequestClient,
    pull_request_source::PullRequestSource, task_transition_run_store::TaskTransitionRunStore,
};
use nexora_platform::*;
use serde_json::{Value, json};
use zeroize::Zeroizing;
pub(crate) struct TaskTransitionProvider;
impl TaskTransitionProvider {
    fn encoded(value: &str) -> String {
        url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
    }
    pub(crate) fn authority(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        repository: &str,
    ) -> Result<Value, PlatformError> {
        let project = project.ok_or(PlatformError::Conflict(
            "Git actions require a project-linked task",
        ))?;
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let fact =
            crate::task_transition_provider_authority::TaskTransitionProviderAuthority::lock(
                &mut tx, ctx, project, repository,
            )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(fact)
    }
    pub(crate) fn source(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        repository: &str,
        policy: &GitNetworkPolicy,
    ) -> Result<(PullRequestSource, Zeroizing<String>), PlatformError> {
        let project = project.ok_or(PlatformError::Conflict(
            "Git actions require a project-linked task",
        ))?;
        let source = PullRequestSource::load(store, ctx, project, repository, policy)?;
        let token = match source.connection.provider {
            GitProvider::Github => {
                let _ = crate::github_repository_proof::GithubRepositoryProof::check(
                    store,
                    &source.connection,
                    source
                        .repository
                        .remote_full_name
                        .as_deref()
                        .ok_or(PlatformError::Storage)?,
                    source.repository.remote_id.as_deref(),
                    policy,
                )?;
                let tokens = crate::github_token_store::GithubTokenStore::new(store)
                    .tokens(&source.connection, policy)?;
                Zeroizing::new(tokens.access_token.to_string())
            }
            GitProvider::Gitlab => {
                let tokens = crate::gitlab_token_store::GitlabTokenStore::new(store)
                    .tokens(&source.connection, policy)?;
                let client = crate::gitlab_client::GitlabClient::new(policy);
                let id = source
                    .repository
                    .remote_id
                    .as_deref()
                    .ok_or(PlatformError::Storage)?;
                if client.user(&tokens.access_token)?.0 != source.connection.remote_account_id
                    || client.repository(&tokens.access_token, id)?.0 != id
                {
                    return Err(PlatformError::Conflict(
                        "Git provider repository identity changed",
                    ));
                }
                Zeroizing::new(tokens.access_token.to_string())
            }
        };
        let current = PullRequestSource::load(store, ctx, project, repository, policy)?;
        if !source.matches(&current) {
            return Err(PlatformError::Conflict(
                "repository authority changed during provider verification",
            ));
        }
        Ok((current, token))
    }
    pub(crate) fn root(source: &PullRequestSource) -> Result<String, PlatformError> {
        if source.connection.provider == GitProvider::Github {
            Ok(format!(
                "/repos/{}",
                source
                    .repository
                    .remote_full_name
                    .as_deref()
                    .ok_or(PlatformError::Storage)?
            ))
        } else {
            let id = source
                .repository
                .remote_id
                .as_deref()
                .ok_or(PlatformError::Storage)?;
            if id.parse::<u64>().ok().filter(|id| *id > 0).is_none() {
                return Err(PlatformError::Storage);
            }
            Ok(format!("/api/v4/projects/{id}"))
        }
    }
    pub(crate) fn sha(
        client: &PullRequestClient,
        source: &PullRequestSource,
        token: &str,
        branch: &str,
    ) -> Result<String, PlatformError> {
        let root = Self::root(source)?;
        let path = if source.connection.provider == GitProvider::Github {
            format!("{root}/branches/{}", Self::encoded(branch))
        } else {
            format!("{root}/repository/branches/{}", Self::encoded(branch))
        };
        let value = client.get(&path, token)?;
        value["commit"][if source.connection.provider == GitProvider::Github {
            "sha"
        } else {
            "id"
        }]
        .as_str()
        .filter(|sha| TaskWorkBranchReport::valid_sha(sha))
        .map(str::to_owned)
        .ok_or(PlatformError::Unavailable)
    }
    pub(crate) fn revision_sha(
        client: &PullRequestClient,
        source: &PullRequestSource,
        token: &str,
        revision: &str,
    ) -> Result<String, PlatformError> {
        if !TaskWorkBranchReport::valid_sha(revision) {
            return Self::sha(client, source, token, revision);
        }
        let root = Self::root(source)?;
        let path = if source.connection.provider == GitProvider::Github {
            format!("{root}/commits/{revision}")
        } else {
            format!("{root}/repository/commits/{revision}")
        };
        let value = client.get(&path, token)?;
        value[if source.connection.provider == GitProvider::Github {
            "sha"
        } else {
            "id"
        }]
        .as_str()
        .filter(|sha| TaskWorkBranchReport::valid_sha(sha))
        .map(str::to_owned)
        .ok_or(PlatformError::Unavailable)
    }
    pub fn verify(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        branch: &TaskTransitionBranch,
        base: Option<&str>,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        if !DevelopmentPolicy::valid_ref(&branch.branch_name)
            || !TaskWorkBranchReport::valid_sha(&branch.expected_head_sha)
        {
            return Err(PlatformError::Validation(
                "invalid branch or expected head SHA",
            ));
        }
        let fact = Self::authority(store, ctx, project, &branch.repository_id)?;
        let (source, token) = Self::source(store, ctx, project, &branch.repository_id, policy)?;
        let client = PullRequestClient::new(policy, source.connection.provider);
        let head = Self::sha(&client, &source, &token, &branch.branch_name)?;
        if head != branch.expected_head_sha {
            return Err(PlatformError::Conflict(
                "provider branch head changed; refresh branch facts and obtain a new approval",
            ));
        }
        let base = if let Some(base) = base {
            base.to_owned()
        } else {
            let mut tx = store
                .client
                .transaction()
                .map_err(PlatformStore::database_error)?;
            let p = crate::development_policy_store::DevelopmentPolicyStore::lock(
                &mut tx,
                project.ok_or(PlatformError::Forbidden)?,
                &branch.repository_id,
            )?;
            tx.commit().map_err(PlatformStore::database_error)?;
            p.base_branch
        };
        let base_sha = Self::revision_sha(&client, &source, &token, &base)?;
        let root = Self::root(&source)?;
        let compared = if source.connection.provider == GitProvider::Github {
            client.get(&format!("{root}/compare/{base_sha}...{head}"), &token)?
        } else {
            client.get(
                &format!("{root}/repository/merge_base?refs[]={base_sha}&refs[]={head}"),
                &token,
            )?
        };
        let ancestor = if source.connection.provider == GitProvider::Github {
            compared["merge_base_commit"]["sha"].as_str() == Some(base_sha.as_str())
        } else {
            compared["id"].as_str() == Some(base_sha.as_str())
        };
        if !ancestor {
            return Err(PlatformError::Conflict(
                "working branch does not contain the required base branch head; update and push it before retrying",
            ));
        }
        let current = PullRequestSource::load(
            store,
            ctx,
            project.ok_or(PlatformError::Forbidden)?,
            &branch.repository_id,
            policy,
        )?;
        if !source.matches(&current) {
            return Err(PlatformError::Conflict(
                "repository authority changed during branch verification",
            ));
        }
        if fact != Self::authority(store, ctx, project, &branch.repository_id)? {
            return Err(PlatformError::Conflict(
                "repository authority or development policy changed after provider verification",
            ));
        }
        store.transition_verified_facts.push(fact);
        Ok(
            json!({"repository_id":branch.repository_id,"branch_name":branch.branch_name,"head_sha":head,"base_branch":base,"base_sha":base_sha,"provenance":"provider_verified"}),
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        branch: &TaskTransitionBranch,
        source_branch: &str,
        target_branch: &str,
        title: &str,
        index: usize,
        policy: &GitNetworkPolicy,
        run: &mut Value,
    ) -> Result<Value, PlatformError> {
        // Pull request creation requires both refs to exist; branch creation is a separate durable step.
        let task = TaskTransitionRunStore::authorize(
            store,
            ctx,
            project,
            run["task_id"].as_str().ok_or(PlatformError::Storage)?,
        )?;
        if task.revision
            != run["task_revision"]
                .as_i64()
                .ok_or(PlatformError::Storage)?
        {
            return Err(PlatformError::Conflict(
                "task revision changed before provider write",
            ));
        }
        let fact = Self::authority(store, ctx, project, &branch.repository_id)?;
        let (source, token) = Self::source(store, ctx, project, &branch.repository_id, policy)?;
        let client = PullRequestClient::new(policy, source.connection.provider);
        let root = Self::root(&source)?;
        let head = Self::sha(&client, &source, &token, source_branch)?;
        if head != branch.expected_head_sha {
            return Err(PlatformError::Conflict(
                "provider branch head changed before pull request creation",
            ));
        }
        let target_sha = Self::sha(&client, &source, &token, target_branch)?;
        let review_description = if run["outcomes"].as_array().is_some_and(|outcomes| {
            outcomes.iter().any(|outcome| {
                outcome["kind"] == "create_code_review"
                    && outcome["result"]["reviews"]
                        .as_array()
                        .is_some_and(|reviews| {
                            reviews
                                .iter()
                                .any(|review| review["repository_id"] == branch.repository_id)
                        })
            })
        }) {
            crate::task_pre_pr_review::TaskPrePrReview::description(
                store,
                ctx,
                project.ok_or(PlatformError::Forbidden)?,
                &task.id,
                task.revision,
                &branch.repository_id,
                &branch.expected_head_sha,
                &target_sha,
            )?
        } else {
            String::new()
        };
        let github = source.connection.provider == GitProvider::Github;
        let endpoint = format!("{root}/{}", if github { "pulls" } else { "merge_requests" });
        let key = format!("{index}:{}", branch.repository_id);
        let marker = format!(
            "<!-- nexora-transition:{}:{key} -->",
            run["id"].as_str().ok_or(PlatformError::Storage)?
        );
        let mut found = None;
        // Reconciliation searches all states, preventing a second PR even if a previous response was lost or the PR closed.
        let mut page = 1u64;
        loop {
            let query = if github {
                format!(
                    "{endpoint}?state=all&head={}&base={}&per_page=100&page={page}",
                    Self::encoded(&format!(
                        "{}:{source_branch}",
                        source
                            .repository
                            .remote_full_name
                            .as_deref()
                            .ok_or(PlatformError::Storage)?
                            .split('/')
                            .next()
                            .ok_or(PlatformError::Storage)?
                    )),
                    Self::encoded(target_branch)
                )
            } else {
                format!(
                    "{endpoint}?state=all&source_branch={}&target_branch={}&per_page=100&page={page}",
                    Self::encoded(source_branch),
                    Self::encoded(target_branch)
                )
            };
            let value = client.get(&query, &token)?;
            let entries = value.as_array().ok_or(PlatformError::Unavailable)?;
            for value in entries {
                if value[if github { "body" } else { "description" }]
                    .as_str()
                    .is_some_and(|body| body.contains(&marker))
                {
                    if found.is_some() {
                        return Err(PlatformError::Conflict(
                            "multiple matching provider requests require administrator reconciliation",
                        ));
                    }
                    found = Some(value.clone());
                }
            }
            if entries.len() < 100 {
                break;
            }
            page = page.checked_add(1).ok_or(PlatformError::Unavailable)?;
        }
        let raw = if let Some(found) = found {
            found
        } else {
            if run["provider_intents"].get(&key).is_some() {
                return Err(PlatformError::Conflict(
                    "previous pull request outcome remains unknown; reconcile it at the provider before retrying",
                ));
            }
            let body = if github {
                json!({"title":title.chars().take(240).collect::<String>(),"head":source_branch,"base":target_branch,"body":format!("Task {}\n\n{marker}{review_description}",run["task_id"].as_str().unwrap_or(""))})
            } else {
                json!({"title":title.chars().take(240).collect::<String>(),"source_branch":source_branch,"target_branch":target_branch,"description":format!("Task {}\n\n{marker}{review_description}",run["task_id"].as_str().unwrap_or("")),"remove_source_branch":false})
            };
            let current_task = TaskTransitionRunStore::authorize(
                store,
                ctx,
                project,
                run["task_id"].as_str().ok_or(PlatformError::Storage)?,
            )?;
            if current_task.revision
                != run["task_revision"]
                    .as_i64()
                    .ok_or(PlatformError::Storage)?
            {
                return Err(PlatformError::Conflict(
                    "task revision changed before provider write",
                ));
            }
            let authority = PullRequestSource::load(
                store,
                ctx,
                project.ok_or(PlatformError::Forbidden)?,
                &branch.repository_id,
                policy,
            )?;
            if !source.matches(&authority) {
                return Err(PlatformError::Conflict(
                    "repository authority changed before provider write",
                ));
            }
            if Self::sha(&client, &source, &token, source_branch)? != branch.expected_head_sha {
                return Err(PlatformError::Conflict(
                    "provider branch head changed before provider write",
                ));
            }
            run["provider_intents"][&key] = json!({"status":"request_started","repository_id":branch.repository_id,"expected_head_sha":branch.expected_head_sha});
            TaskTransitionRunStore::save(store, run)?;
            match client.post(&endpoint, &token, &body) {
                Ok(value) => value,
                Err(
                    error @ (PlatformError::Forbidden
                    | PlatformError::NotFound
                    | PlatformError::Validation(_)
                    | PlatformError::Conflict(_)),
                ) => {
                    // Only an explicit provider rejection proves that no PR was created.
                    run["provider_intents"]
                        .as_object_mut()
                        .ok_or(PlatformError::Storage)?
                        .remove(&key);
                    TaskTransitionRunStore::save(store, run)?;
                    return Err(error);
                }
                Err(error) => return Err(error),
            }
        };
        let summary = crate::pull_request_decoder::PullRequestDecoder::summary(
            source.connection.provider,
            &raw,
        )?;
        if summary["head_sha"] != branch.expected_head_sha
            || summary["source_branch"] != source_branch
            || summary["target_branch"] != target_branch
        {
            return Err(PlatformError::Conflict(
                "pull request head or branches changed; refresh facts and approval",
            ));
        }
        if summary["state"] != "open" {
            return Err(PlatformError::Conflict(
                "reconciled pull request is no longer open; review it before starting another transition",
            ));
        }
        let current = PullRequestSource::load(
            store,
            ctx,
            project.ok_or(PlatformError::Forbidden)?,
            &branch.repository_id,
            policy,
        )?;
        if !source.matches(&current) {
            return Err(PlatformError::Conflict(
                "repository authority changed during pull request creation",
            ));
        }
        if fact != Self::authority(store, ctx, project, &branch.repository_id)? {
            return Err(PlatformError::Conflict(
                "repository authority or development policy changed after provider verification",
            ));
        }
        store.transition_verified_facts.push(fact);
        run["provider_intents"][&key] = json!({"status":"confirmed","repository_id":branch.repository_id,"number":summary["number"],"head_sha":summary["head_sha"]});
        TaskTransitionRunStore::save(store, run)?;
        Ok(summary)
    }
}
impl PlatformStore {
    pub fn task_transition_branch_fact(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        repository: &str,
        branch: &str,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        if !DevelopmentPolicy::valid_ref(branch) {
            return Err(PlatformError::Validation("invalid branch name"));
        }
        let project = self.pool_task_scope(ctx, id, Capability::TasksRead)?;
        // Reading provider facts requires task visibility as well as the existing repository binding authority.
        self.pool_task(ctx, id)?;
        let (source, token) =
            TaskTransitionProvider::source(self, ctx, project.as_deref(), repository, policy)?;
        let client = PullRequestClient::new(policy, source.connection.provider);
        let sha = TaskTransitionProvider::sha(&client, &source, &token, branch)?;
        let current = PullRequestSource::load(
            self,
            ctx,
            project.as_deref().ok_or(PlatformError::Forbidden)?,
            repository,
            policy,
        )?;
        if !source.matches(&current) {
            return Err(PlatformError::Conflict(
                "repository authority changed during provider request",
            ));
        }
        Ok(
            json!({"repository_id":repository,"branch_name":branch,"expected_head_sha":sha,"provenance":"provider_verified"}),
        )
    }
}

impl TaskTransitionProvider {
    #[allow(clippy::too_many_arguments)]
    pub fn create_branch(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        branch: &TaskTransitionBranch,
        base_branch: &str,
        expected_base_sha: &str,
        index: usize,
        policy: &GitNetworkPolicy,
        run: &mut Value,
    ) -> Result<Value, PlatformError> {
        use crate::task_provider_operations::TaskProviderOperations;
        if !DevelopmentPolicy::valid_ref(&branch.branch_name)
            || !DevelopmentPolicy::valid_ref(base_branch)
            || !TaskWorkBranchReport::valid_sha(expected_base_sha)
            || branch.expected_head_sha != expected_base_sha
            || branch.branch_name == base_branch
        {
            return Err(PlatformError::Validation(
                "branch creation requires a distinct branch and exact pinned base SHA",
            ));
        }
        let task = TaskTransitionRunStore::authorize(
            store,
            ctx,
            project,
            run["task_id"].as_str().ok_or(PlatformError::Storage)?,
        )?;
        if Some(task.revision) != run["task_revision"].as_i64() {
            return Err(PlatformError::Conflict(
                "task revision changed before branch creation",
            ));
        }
        let fact = Self::authority(store, ctx, project, &branch.repository_id)?;
        let (source, token) = Self::source(store, ctx, project, &branch.repository_id, policy)?;
        let client = PullRequestClient::new(policy, source.connection.provider);
        let root = Self::root(&source)?;
        let actual_base = if TaskWorkBranchReport::valid_sha(base_branch) {
            let path = if source.connection.provider == GitProvider::Github {
                format!("{root}/commits/{base_branch}")
            } else {
                format!("{root}/repository/commits/{base_branch}")
            };
            let commit = client.get(&path, &token)?;
            commit[if source.connection.provider == GitProvider::Github {
                "sha"
            } else {
                "id"
            }]
            .as_str()
            .filter(|sha| TaskWorkBranchReport::valid_sha(sha))
            .ok_or(PlatformError::Unavailable)?
            .to_owned()
        } else {
            Self::sha(&client, &source, &token, base_branch)?
        };
        let key = format!(
            "branch:{index}:{}:{}",
            branch.repository_id, branch.branch_name
        );
        let existing = Self::sha(&client, &source, &token, &branch.branch_name);
        if actual_base != expected_base_sha
            && !(run["provider_intents"].get(&key).is_some()
                && existing.as_ref().is_ok_and(|sha| sha == expected_base_sha))
        {
            return Err(PlatformError::Conflict(
                "provider base branch changed; refresh pinned branch creation facts",
            ));
        }
        match existing {
            Ok(sha) if sha == expected_base_sha => {}
            Ok(_) => {
                return Err(PlatformError::Conflict(
                    "existing branch has a different SHA; automatic overwrite is forbidden",
                ));
            }
            Err(PlatformError::NotFound) => {
                if run["provider_intents"].get(&key).is_some() {
                    return Err(PlatformError::Conflict(
                        "previous branch creation outcome remains unknown; reconcile it at the provider before retrying",
                    ));
                }
                let current_task =
                    TaskTransitionRunStore::authorize(store, ctx, project, &task.id)?;
                if current_task.revision != task.revision
                    || fact != Self::authority(store, ctx, project, &branch.repository_id)?
                {
                    return Err(PlatformError::Conflict(
                        "task or repository authority changed before branch creation",
                    ));
                }
                run["provider_intents"][&key] = json!({"status":"request_started","repository_id":branch.repository_id,"branch_name":branch.branch_name,"base_branch":base_branch,"base_sha":expected_base_sha});
                TaskTransitionRunStore::save(store, run)?;
                let result = TaskProviderOperations::create_branch(
                    &client,
                    source.connection.provider,
                    &root,
                    &token,
                    &branch.branch_name,
                    expected_base_sha,
                );
                // Reconcile even explicit rejection: another request may have created the same exact ref.
                match result {
                    Ok(()) => {}
                    Err(error) => match Self::sha(&client, &source, &token, &branch.branch_name) {
                        Ok(sha) if sha == expected_base_sha => {}
                        _ => return Err(error),
                    },
                }
            }
            Err(error) => return Err(error),
        }
        if Self::sha(&client, &source, &token, &branch.branch_name)? != expected_base_sha {
            return Err(PlatformError::Conflict(
                "provider branch changed after creation",
            ));
        }
        let current = PullRequestSource::load(
            store,
            ctx,
            project.ok_or(PlatformError::Forbidden)?,
            &branch.repository_id,
            policy,
        )?;
        if !source.matches(&current)
            || fact != Self::authority(store, ctx, project, &branch.repository_id)?
        {
            return Err(PlatformError::Conflict(
                "repository authority changed during branch creation",
            ));
        }
        let result = json!({"repository_id":branch.repository_id,"branch_name":branch.branch_name,"head_sha":expected_base_sha,"base_branch":base_branch,"base_sha":expected_base_sha,"provenance":"provider_verified"});
        run["provider_intents"][&key] = json!({"status":"confirmed","result":result});
        TaskTransitionRunStore::save(store, run)?;
        store.transition_verified_facts.push(fact);
        Ok(result)
    }
    pub fn observe(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        branch: &TaskTransitionBranch,
        observation: &str,
        selector: Option<&str>,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        Self::observe_to(
            store,
            ctx,
            project,
            branch,
            observation,
            selector,
            None,
            policy,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn observe_to(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        branch: &TaskTransitionBranch,
        observation: &str,
        selector: Option<&str>,
        target_branch: Option<&str>,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        use crate::task_provider_operations::TaskProviderOperations;
        if !DevelopmentPolicy::valid_ref(&branch.branch_name)
            || !TaskWorkBranchReport::valid_sha(&branch.expected_head_sha)
        {
            return Err(PlatformError::Validation(
                "invalid pinned provider observation",
            ));
        }
        let fact = Self::authority(store, ctx, project, &branch.repository_id)?;
        let (source, token) = Self::source(store, ctx, project, &branch.repository_id, policy)?;
        let client = PullRequestClient::new(policy, source.connection.provider);
        let root = Self::root(&source)?;
        let result = match observation {
            "pipeline" | "commit_pipeline" => {
                if observation == "pipeline"
                    && Self::sha(&client, &source, &token, &branch.branch_name)?
                        != branch.expected_head_sha
                {
                    return Err(PlatformError::Conflict(
                        "provider branch changed before pipeline verification",
                    ));
                }
                TaskProviderOperations::pipeline(
                    &client,
                    source.connection.provider,
                    &root,
                    &token,
                    &branch.expected_head_sha,
                    selector,
                )?
            }
            "merged_pipeline" => {
                let merged = TaskProviderOperations::merged_to(
                    &client,
                    source.connection.provider,
                    &root,
                    &token,
                    &branch.branch_name,
                    &branch.expected_head_sha,
                    target_branch,
                )?;
                let sha = merged["merge_sha"]
                    .as_str()
                    .ok_or(PlatformError::Unavailable)?;
                let mut checks = TaskProviderOperations::pipeline(
                    &client,
                    source.connection.provider,
                    &root,
                    &token,
                    sha,
                    selector,
                )?;
                checks["source_head_sha"] = json!(branch.expected_head_sha);
                checks["merge"] = merged;
                checks
            }
            "pull_request_merged" => TaskProviderOperations::merged_to(
                &client,
                source.connection.provider,
                &root,
                &token,
                &branch.branch_name,
                &branch.expected_head_sha,
                target_branch.or(selector),
            )?,
            "deployment" => {
                let environment = selector.ok_or(PlatformError::Validation(
                    "deployment environment is required",
                ))?;
                // A deployed revision is the provider-confirmed merge result, never an arbitrary client-supplied SHA.
                let merged = TaskProviderOperations::merged_to(
                    &client,
                    source.connection.provider,
                    &root,
                    &token,
                    &branch.branch_name,
                    &branch.expected_head_sha,
                    target_branch,
                )?;
                let sha = merged["merge_sha"]
                    .as_str()
                    .ok_or(PlatformError::Unavailable)?;
                let mut deployment = TaskProviderOperations::deployment(
                    &client,
                    source.connection.provider,
                    &root,
                    &token,
                    sha,
                    environment,
                )?;
                deployment["source_head_sha"] = json!(branch.expected_head_sha);
                deployment["merge"] = merged;
                deployment
            }
            _ => {
                return Err(PlatformError::Validation(
                    "unsupported provider observation",
                ));
            }
        };
        let current = PullRequestSource::load(
            store,
            ctx,
            project.ok_or(PlatformError::Forbidden)?,
            &branch.repository_id,
            policy,
        )?;
        if !source.matches(&current)
            || fact != Self::authority(store, ctx, project, &branch.repository_id)?
        {
            return Err(PlatformError::Conflict(
                "repository authority changed during provider observation",
            ));
        }
        store.transition_verified_facts.push(fact);
        let mut result = result;
        result["repository_id"] = json!(branch.repository_id);
        result["source_branch"] = json!(branch.branch_name);
        Ok(result)
    }
}

impl TaskTransitionProvider {
    pub fn review(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        branch: &TaskTransitionBranch,
        base: Option<&str>,
        policy: &GitNetworkPolicy,
        run: &mut Value,
    ) -> Result<Value, PlatformError> {
        let mut review = crate::task_pre_pr_review::TaskPrePrReview::run(
            store, ctx, project, branch, base, policy, run,
        )?;
        review["repository_id"] = json!(branch.repository_id);
        Ok(review)
    }
}

impl TaskTransitionProvider {
    pub(crate) fn verify_approval(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        task: &WorkTask,
        run: &Value,
    ) -> Result<(), PlatformError> {
        let parameters: Value = store
            .client
            .query_one(
                "SELECT parameters FROM task_transition_runs WHERE id=$1",
                &[&run["id"].as_str().ok_or(PlatformError::Storage)?],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        let input: TaskTransitionRequest = PlatformStore::decode(parameters)?;
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let version = run["workflow_version_id"]
            .as_str()
            .ok_or(PlatformError::Storage)?;
        let flow = crate::task_pool_workflow::TaskPoolWorkflow::flow(
            &mut tx,
            task.project_id.as_deref(),
            version,
            false,
        )?;
        let approved = crate::task_approval_store::TaskApprovalStore::consume(
            &mut tx,
            ctx,
            task,
            &flow,
            run["transition_id"]
                .as_str()
                .ok_or(PlatformError::Storage)?,
            &input,
        )?;
        tx.rollback().map_err(PlatformStore::database_error)?;
        if !approved {
            return Err(PlatformError::Conflict("human approval is required"));
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn merge(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        branch: &TaskTransitionBranch,
        target: &str,
        index: usize,
        policy: &GitNetworkPolicy,
        run: &mut Value,
    ) -> Result<Value, PlatformError> {
        use crate::task_provider_merge::TaskProviderMerge;
        if !DevelopmentPolicy::valid_ref(&branch.branch_name)
            || !DevelopmentPolicy::valid_ref(target)
            || branch.branch_name == target
            || !TaskWorkBranchReport::valid_sha(&branch.expected_head_sha)
        {
            return Err(PlatformError::Validation(
                "merge requires distinct valid branches and an exact head SHA",
            ));
        }
        let task = TaskTransitionRunStore::authorize(
            store,
            ctx,
            project,
            run["task_id"].as_str().ok_or(PlatformError::Storage)?,
        )?;
        if Some(task.revision) != run["task_revision"].as_i64() {
            return Err(PlatformError::Conflict(
                "task revision changed before merge",
            ));
        }
        let fact = Self::authority(store, ctx, project, &branch.repository_id)?;
        let (source, token) = Self::source(store, ctx, project, &branch.repository_id, policy)?;
        let client = PullRequestClient::new(policy, source.connection.provider);
        let root = Self::root(&source)?;
        let raw = TaskProviderMerge::find(
            &client,
            source.connection.provider,
            &root,
            &token,
            &branch.branch_name,
            &branch.expected_head_sha,
            target,
        )?;
        let key = format!("merge:{index}:{}:{target}", branch.repository_id);
        let result = if let Some(result) = TaskProviderMerge::confirmed(
            source.connection.provider,
            &raw,
            &branch.expected_head_sha,
            target,
        )? {
            result
        } else {
            if run["provider_intents"].get(&key).is_some() {
                return Err(PlatformError::Conflict(
                    "previous merge outcome remains unknown; reconcile the provider before another merge request",
                ));
            }
            Self::verify_approval(store, ctx, &task, run)?;
            let readiness = TaskProviderMerge::ready(
                &client,
                source.connection.provider,
                &root,
                &token,
                &raw,
                &branch.expected_head_sha,
                target,
            )?;
            let current = TaskTransitionRunStore::authorize(store, ctx, project, &task.id)?;
            if current.revision != task.revision
                || fact != Self::authority(store, ctx, project, &branch.repository_id)?
                || Self::sha(&client, &source, &token, &branch.branch_name)?
                    != branch.expected_head_sha
            {
                return Err(PlatformError::Conflict(
                    "task, repository authority or source head changed before merge",
                ));
            }
            Self::verify_approval(store, ctx, &task, run)?;
            let number = raw[if source.connection.provider == GitProvider::Github {
                "number"
            } else {
                "iid"
            }]
            .as_u64()
            .ok_or(PlatformError::Unavailable)?;
            run["provider_intents"][&key] = json!({"status":"request_started","repository_id":branch.repository_id,"head_sha":branch.expected_head_sha,"target_branch":target,"number":number,"provider_readiness":readiness});
            TaskTransitionRunStore::save(store, run)?;
            let attempted = TaskProviderMerge::write(
                &client,
                source.connection.provider,
                &root,
                &token,
                number,
                &branch.expected_head_sha,
            );
            // Never blindly retry an unknown write. A read of this exact PR is the receipt.
            let observed = TaskProviderMerge::find(
                &client,
                source.connection.provider,
                &root,
                &token,
                &branch.branch_name,
                &branch.expected_head_sha,
                target,
            )?;
            match TaskProviderMerge::confirmed(
                source.connection.provider,
                &observed,
                &branch.expected_head_sha,
                target,
            )? {
                Some(result) => result,
                None => {
                    if let Err(error) = attempted {
                        if matches!(
                            error,
                            PlatformError::Forbidden
                                | PlatformError::NotFound
                                | PlatformError::Validation(_)
                                | PlatformError::Conflict(_)
                        ) {
                            run["provider_intents"]
                                .as_object_mut()
                                .ok_or(PlatformError::Storage)?
                                .remove(&key);
                            TaskTransitionRunStore::save(store, run)?;
                        }
                        return Err(error);
                    }
                    return Err(PlatformError::Conflict(
                        "waiting for the provider to confirm the merge; its outcome must be reconciled before retrying",
                    ));
                }
            }
        };
        if fact != Self::authority(store, ctx, project, &branch.repository_id)? {
            return Err(PlatformError::Conflict(
                "repository authority changed during merge",
            ));
        }
        let mut result = result;
        result["repository_id"] = json!(branch.repository_id);
        result["source_branch"] = json!(branch.branch_name);
        run["provider_intents"][&key] = json!({"status":"confirmed","result":result});
        TaskTransitionRunStore::save(store, run)?;
        store.transition_verified_facts.push(fact);
        Ok(result)
    }
}
impl TaskTransitionProvider {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn release_commit(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        commit: &str,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        if !TaskWorkBranchReport::valid_sha(commit) {
            return Err(PlatformError::Validation(
                "task commit requires a full commit SHA",
            ));
        }
        // A task commit is repository-scoped evidence, not branch-scoped evidence.
        // Provider metadata verifies the full SHA against the selected repository.
        let (source, token) = Self::source(store, ctx, Some(project), repository, policy)?;
        let root = Self::root(&source)?;
        let client = PullRequestClient::new(policy, source.connection.provider);
        let metadata = client.get(
            &if source.connection.provider == GitProvider::Github {
                format!("{root}/commits/{commit}")
            } else {
                format!("{root}/repository/commits/{commit}")
            },
            &token,
        ).map_err(|error| match error {
            PlatformError::NotFound => PlatformError::Validation(
                "commit SHA was not found in the selected repository; verify the repository and full SHA",
            ),
            other => other,
        })?;
        let actual = if source.connection.provider == GitProvider::Github {
            metadata["sha"].as_str()
        } else {
            metadata["id"].as_str()
        };
        if actual != Some(commit) {
            return Err(PlatformError::Conflict(
                "provider returned another commit identity",
            ));
        }
        let parents = if source.connection.provider == GitProvider::Github {
            metadata["parents"].as_array()
        } else {
            metadata["parent_ids"].as_array()
        }
        .ok_or(PlatformError::Unavailable)?;
        if parents.len() != 1 {
            return Err(PlatformError::Validation(
                "merge commits cannot be linked; select an individual commit with one parent",
            ));
        }
        let fact = Self::authority(store, ctx, Some(project), repository)?;
        Ok(json!({"commit_sha":commit,"provenance":"provider_verified","authority":fact}))
    }
    pub(crate) fn release_transport(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        policy: &GitNetworkPolicy,
    ) -> Result<
        (
            PullRequestSource,
            nexora_source::GitFetchCredentials,
            String,
        ),
        PlatformError,
    > {
        let (source, token) = Self::source(store, ctx, Some(project), repository, policy)?;
        let client = PullRequestClient::new(policy, source.connection.provider);
        let main = Self::sha(&client, &source, &token, "main")?;
        let credentials = nexora_source::GitFetchCredentials::new(
            if source.connection.provider == GitProvider::Github {
                "x-access-token"
            } else {
                "oauth2"
            },
            token.to_string(),
        )
        .map_err(|_| PlatformError::Unavailable)?;
        Ok((source, credentials, main))
    }
    pub(crate) fn release_ref(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        branch: &str,
        policy: &GitNetworkPolicy,
    ) -> Result<Option<String>, PlatformError> {
        let (source, token) = Self::source(store, ctx, Some(project), repository, policy)?;
        let client = PullRequestClient::new(policy, source.connection.provider);
        match Self::sha(&client, &source, &token, branch) {
            Ok(sha) => Ok(Some(sha)),
            Err(PlatformError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }
}
