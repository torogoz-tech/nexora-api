use crate::{
    GitNetworkPolicy, PlatformStore, SecretGenerator,
    task_transition_provider::TaskTransitionProvider,
    task_transition_run_store::TaskTransitionRunStore, task_transition_store::TaskTransitionStore,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct TaskTransitionEngine;
impl TaskTransitionEngine {
    pub fn execute(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        id: &str,
        transition: &str,
        input: &TaskTransitionRequest,
        policy: &GitNetworkPolicy,
    ) -> Result<WorkTask, PlatformError> {
        TaskTransitionRunStore::authorize(store, ctx, project, id)?;
        // Session advisory lock serializes execution without keeping a database transaction open during HTTP.
        let lock = format!("task-transition:{id}");
        let acquired: bool = store
            .client
            .query_one(
                "SELECT pg_try_advisory_lock(hashtextextended($1,0))",
                &[&lock],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        if !acquired {
            return Err(PlatformError::Conflict(
                "task transition is already executing; inspect its run and retry",
            ));
        }
        let receipt_lock = store
            .request_context
            .as_ref()
            .map(|request| {
                serde_json::to_string(&(
                    &store.namespace,
                    ctx.actor_id(),
                    &request.operation,
                    &request.request_id,
                ))
                .map_err(|_| PlatformError::Storage)
            })
            .transpose()?;
        if let Some(receipt_lock) = &receipt_lock {
            store
                .client
                .query_one(
                    "SELECT pg_advisory_lock(hashtextextended($1,0))",
                    &[receipt_lock],
                )
                .map_err(PlatformStore::database_error)?;
        }
        store.transition_verified_facts.clear();
        let executed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let request = store.request_context.clone();
            if let Some(request) = request.as_ref() {
                let mut tx = store
                    .client
                    .transaction()
                    .map_err(PlatformStore::database_error)?;
                if let Some(value) = crate::receipt_writer::ReceiptWriter::read(
                    &mut tx,
                    &store.namespace,
                    Some(request),
                    ctx.actor_id(),
                    &request.operation,
                )? {
                    tx.commit().map_err(PlatformStore::database_error)?;
                    return PlatformStore::decode(value);
                }
                tx.commit().map_err(PlatformStore::database_error)?;
            }
            let result = Self::locked(store, ctx, project, id, transition, input, policy)?;
            if let Some(request) = request.as_ref() {
                let mut tx = store
                    .client
                    .transaction()
                    .map_err(PlatformStore::database_error)?;
                crate::receipt_writer::ReceiptWriter::read(
                    &mut tx,
                    &store.namespace,
                    Some(request),
                    ctx.actor_id(),
                    &request.operation,
                )?;
                crate::receipt_writer::ReceiptWriter::save(&mut tx, Some(request), &json!(result))?;
                tx.commit().map_err(PlatformStore::database_error)?;
            }
            Ok(result)
        }));
        store.transition_verified_facts.clear();
        store.transition_run_checkpoint = None;
        if let Some(receipt_lock) = receipt_lock {
            store
                .client
                .query_one(
                    "SELECT pg_advisory_unlock(hashtextextended($1,0))",
                    &[&receipt_lock],
                )
                .map_err(PlatformStore::database_error)?;
        }
        store
            .client
            .query_one(
                "SELECT pg_advisory_unlock(hashtextextended($1,0))",
                &[&lock],
            )
            .map_err(PlatformStore::database_error)?;
        match executed {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
    fn locked(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        id: &str,
        transition: &str,
        input: &TaskTransitionRequest,
        policy: &GitNetworkPolicy,
    ) -> Result<WorkTask, PlatformError> {
        let fingerprint = SecretGenerator::checksum(
            &serde_json::to_vec(
                &crate::task_approval_fingerprint::TaskApprovalFingerprint::parameters(input)?,
            )
            .map_err(|_| PlatformError::Storage)?,
        );
        let row=store.client.query_opt("SELECT record FROM task_transition_runs WHERE task_id=$1 AND task_revision=$2 AND transition_id=$3 AND actor_id=$4 AND fingerprint=$5",&[&id,&input.expected_revision,&transition,&ctx.actor_id(),&fingerprint]).map_err(PlatformStore::database_error)?;
        let mut run = if let Some(row) = row {
            row.get::<_, Value>(0)
        } else {
            let run = json!({"id":SecretGenerator::id("trn")?,"task_id":id,"actor_id":ctx.actor_id(),"request_id":store.request_context.as_ref().map(|request|&request.request_id),"request_operation":store.request_context.as_ref().map(|request|&request.operation),"transition_id":transition,"task_revision":input.expected_revision,"status":"running","next_step":0,"outcomes":[],"provider_intents":{},"advanced":false});
            store.client.execute("INSERT INTO task_transition_runs(id,task_id,project_id,actor_id,task_revision,transition_id,fingerprint,parameters,record,credential_id,auth_method) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",&[&run["id"].as_str(),&id,&project,&ctx.actor_id(),&input.expected_revision,&transition,&fingerprint,&json!(input),&run,&ctx.credential_id(),&if ctx.method()==AuthMethod::ApiKey{"api_key"}else{"session"}]).map_err(PlatformStore::database_error)?;
            run
        };
        if run["status"] == "succeeded" {
            let same_request = store.request_context.as_ref().is_some_and(|request| {
                run["request_id"].as_str() == Some(request.request_id.as_str())
                    && run["request_operation"].as_str() == Some(request.operation.as_str())
            });
            if same_request {
                return PlatformStore::decode(run["result"].clone());
            }
            return Err(PlatformError::Conflict(
                "task revision changed; inspect or retry the existing transition run",
            ));
        }
        // A newly granted approval may be attached without changing any approved operation parameters.
        store
            .client
            .execute(
                "UPDATE task_transition_runs SET parameters=$2 WHERE id=$1",
                &[&run["id"].as_str(), &json!(input)],
            )
            .map_err(PlatformStore::database_error)?;
        run["status"] = json!("running");
        run["error_code"] = Value::Null;
        run["message"] = Value::Null;
        TaskTransitionRunStore::save(store, &run)?;
        let result = Self::resume(store, ctx, project, id, transition, input, policy, &mut run);
        if let Err(ref error) = result {
            TaskTransitionRunStore::fail(store, ctx, &mut run, error)?;
            if run["status"] == "waiting_approval" {
                Self::request_approval(store, ctx, project, id, transition, input, &mut run)?;
            }
        }
        result
    }
    #[allow(clippy::too_many_arguments)]
    fn resume(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        id: &str,
        transition: &str,
        input: &TaskTransitionRequest,
        policy: &GitNetworkPolicy,
        run: &mut Value,
    ) -> Result<WorkTask, PlatformError> {
        let task = TaskTransitionRunStore::authorize(store, ctx, project, id)?;
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let version = run["workflow_version_id"]
            .as_str()
            .or(task.workflow_version_id.as_deref())
            .ok_or(PlatformError::Conflict(
                "task must be taken and resumed before transitioning",
            ))?;
        let flow =
            crate::task_pool_workflow::TaskPoolWorkflow::flow(&mut tx, project, version, false)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        flow.graph.validate()?;
        let edge = flow
            .graph
            .transitions
            .iter()
            .find(|edge| edge.id == transition)
            .ok_or(PlatformError::Conflict(
                "transition is outside pinned workflow",
            ))?
            .clone();
        run["workflow_version_id"] = json!(flow.id);
        let steps = if edge.steps.is_empty() {
            vec![WorkflowTransitionStep::AdvanceState]
        } else {
            edge.steps.clone()
        };
        if steps
            .iter()
            .any(|step| matches!(step, WorkflowTransitionStep::PublishRelease))
        {
            crate::task_release_publication::TaskReleasePublication::selection(
                store, ctx, project, input, run,
            )?;
        } else if input.release_id.is_some() || input.release_revision.is_some() {
            return Err(PlatformError::Validation(
                "release parameters require a publish_release final pipeline",
            ));
        }
        let required_base = flow
            .graph
            .states
            .iter()
            .find(|state| state.id == edge.to)
            .and_then(|state| state.required_base_branch.as_deref())
            .or_else(|| {
                flow.graph
                    .states
                    .iter()
                    .find(|state| state.id == edge.from)
                    .and_then(|state| state.required_base_branch.as_deref())
            });
        if run["advanced"] != true {
            Self::preflight(store, ctx, project, id, transition, input, &flow)?;
            if flow
                .graph
                .states
                .iter()
                .any(|state| state.id == edge.to && state.terminal)
            {
                crate::task_subtask_integration::TaskSubtaskIntegration::verify(
                    store, ctx, &task, policy,
                )?;
            }
            if let Some(base) = required_base {
                if input.branches.is_empty() {
                    return Err(PlatformError::Conflict(
                        "state requires repository branches verified from its required base branch",
                    ));
                }
                for branch in &input.branches {
                    let created_here=steps.iter().any(|step|matches!(step,WorkflowTransitionStep::CreateBranch{repository_id,..} if repository_id.as_deref().is_none_or(|r|r==branch.repository_id)));
                    if !created_here {
                        Self::verify_branch(
                            store,
                            ctx,
                            project,
                            id,
                            branch,
                            Some(base),
                            policy,
                            run,
                        )?;
                    }
                }
            }
        }
        let start = run["next_step"].as_u64().unwrap_or(0) as usize;
        for (index, step) in steps.iter().enumerate().skip(start) {
            TaskTransitionRunStore::authorize(store, ctx, project, id)?;
            let outcome = match step {
                WorkflowTransitionStep::RequireApproval => {
                    TaskTransitionProvider::verify_approval(store, ctx, &task, run)?;
                    json!({"approval_id":input.approval_id})
                }
                WorkflowTransitionStep::VerifyPush { repository_id } => {
                    let branches = Self::branches(input, repository_id.as_deref())?;
                    let mut results = Vec::new();
                    for branch in branches {
                        results.push(Self::verify_branch(
                            store, ctx, project, id, branch, None, policy, run,
                        )?);
                    }
                    json!({"branches":results})
                }
                WorkflowTransitionStep::CreatePullRequest {
                    repository_id,
                    source_branch,
                    target_branch,
                    source_from_task,
                    target_from_parent,
                } => {
                    let results = Self::create_requests(
                        store,
                        ctx,
                        project,
                        &task,
                        input,
                        repository_id.as_deref(),
                        source_branch,
                        target_branch,
                        *source_from_task,
                        *target_from_parent,
                        index,
                        false,
                        policy,
                        run,
                    )?;
                    json!({"pull_requests":results})
                }
                WorkflowTransitionStep::MergePullRequest {
                    repository_id,
                    target_branch,
                    target_from_parent,
                } => {
                    let mut results = Vec::new();
                    for branch in Self::branches(input, repository_id.as_deref())? {
                        let target = crate::task_branch_selectors::TaskBranchSelectors::target(
                            store,
                            &task,
                            &branch.repository_id,
                            Some(target_branch),
                            *target_from_parent,
                        )?
                        .ok_or(PlatformError::Storage)?;
                        results.push(TaskTransitionProvider::merge(
                            store, ctx, project, branch, &target, index, policy, run,
                        )?);
                    }
                    json!({"pull_requests":results})
                }
                WorkflowTransitionStep::GenerateContributorRatings { repository_id } => {
                    let mut generated = 0u64;
                    let mut unrated = 0u64;
                    for branch in Self::branches(input, repository_id.as_deref())? {
                        let result = crate::task_contributor_rating::TaskContributorRating::run(
                            store,
                            ctx,
                            project,
                            branch,
                            required_base,
                            policy,
                            run,
                        )?;
                        generated += result["generated_count"].as_u64().unwrap_or(0);
                        unrated += result["unrated_count"].as_u64().unwrap_or(0);
                    }
                    json!({"status":"completed","generated_count":generated,"unrated_count":unrated})
                }
                WorkflowTransitionStep::CreateCodeReview { repository_id } => {
                    let mut results = Vec::new();
                    for branch in Self::branches(input, repository_id.as_deref())? {
                        let upcoming_target =
                            steps.iter().skip(index + 1).find_map(|step| match step {
                                WorkflowTransitionStep::CreatePullRequest {
                                    repository_id,
                                    target_branch,
                                    target_from_parent,
                                    ..
                                } if repository_id
                                    .as_deref()
                                    .is_none_or(|r| r == branch.repository_id) =>
                                {
                                    Some((target_branch, *target_from_parent))
                                }
                                _ => None,
                            });
                        let review_base = if let Some((literal, parent)) = upcoming_target {
                            crate::task_branch_selectors::TaskBranchSelectors::target(
                                store,
                                &task,
                                &branch.repository_id,
                                Some(literal),
                                parent,
                            )?
                        } else {
                            required_base.map(str::to_owned)
                        };
                        results.push(TaskTransitionProvider::review(
                            store,
                            ctx,
                            project,
                            branch,
                            review_base.as_deref(),
                            policy,
                            run,
                        )?);
                    }
                    json!({"reviews":results})
                }
                WorkflowTransitionStep::CreateBranch {
                    repository_id,
                    base_branch,
                } => {
                    let mut results = Vec::new();
                    for branch in Self::branches(input, repository_id.as_deref())? {
                        let (base, mut sha) = crate::task_branch_origin::TaskBranchOrigin::resolve(
                            store,
                            id,
                            &branch.repository_id,
                            base_branch,
                            &branch.expected_head_sha,
                        )?;
                        if !TaskWorkBranchReport::valid_sha(&base) {
                            sha = store.task_transition_branch_fact(
                                ctx,
                                id,
                                &branch.repository_id,
                                &base,
                                policy,
                            )?["expected_head_sha"]
                                .as_str()
                                .ok_or(PlatformError::Unavailable)?
                                .to_owned();
                        }
                        if branch.expected_head_sha != sha {
                            return Err(PlatformError::Conflict(
                                "expected branch creation SHA differs from the parent or tested revision; refresh provider facts before retrying",
                            ));
                        }
                        let prepared = branch.clone();
                        let evidence = TaskTransitionProvider::create_branch(
                            store, ctx, project, &prepared, &base, &sha, index, policy, run,
                        )?;
                        crate::task_branch_origin::TaskBranchOrigin::save(
                            store, id, &prepared, &base, &sha, &evidence,
                        )?;
                        if !run["branch_bases"].is_object() {
                            run["branch_bases"] = json!({});
                        }
                        run["branch_bases"]
                            [format!("{}:{}", prepared.repository_id, prepared.branch_name)] =
                            json!(sha);
                        results.push(evidence);
                    }
                    json!({"branches":results})
                }
                WorkflowTransitionStep::VerifyPipeline {
                    repository_id,
                    workflow,
                } => {
                    let mut results = Vec::new();
                    for branch in Self::branches(input, repository_id.as_deref())? {
                        results.push(Self::pipeline(
                            store,
                            ctx,
                            project,
                            branch,
                            workflow.as_deref(),
                            index,
                            policy,
                            run,
                        )?);
                    }
                    json!({"checks":results})
                }
                WorkflowTransitionStep::VerifyPullRequestMerged {
                    repository_id,
                    target_branch,
                    target_from_parent,
                } => {
                    let mut results = Vec::new();
                    for branch in Self::branches(input, repository_id.as_deref())? {
                        let target = crate::task_branch_selectors::TaskBranchSelectors::target(
                            store,
                            &task,
                            &branch.repository_id,
                            target_branch.as_deref(),
                            *target_from_parent,
                        )?;
                        results.push(TaskTransitionProvider::observe_to(
                            store,
                            ctx,
                            project,
                            branch,
                            "pull_request_merged",
                            None,
                            target.as_deref(),
                            policy,
                        )?);
                    }
                    json!({"pull_requests":results})
                }
                WorkflowTransitionStep::VerifyDeployment {
                    repository_id,
                    environment,
                    target_branch,
                    target_from_parent,
                } => {
                    let mut results = Vec::new();
                    for branch in Self::branches(input, repository_id.as_deref())? {
                        let target = crate::task_branch_selectors::TaskBranchSelectors::target(
                            store,
                            &task,
                            &branch.repository_id,
                            target_branch.as_deref(),
                            *target_from_parent,
                        )?;
                        results.push(TaskTransitionProvider::observe_to(
                            store,
                            ctx,
                            project,
                            branch,
                            "deployment",
                            Some(environment),
                            target.as_deref(),
                            policy,
                        )?);
                    }
                    json!({"deployments":results})
                }
                WorkflowTransitionStep::PublishRelease => {
                    let publication =
                        crate::task_release_publication::TaskReleasePublication::execute(
                            store,
                            ctx,
                            project,
                            input
                                .release_id
                                .as_deref()
                                .ok_or(PlatformError::Validation(
                                    "select a release before running its final pipeline",
                                ))?,
                            input
                                .release_revision
                                .ok_or(PlatformError::Validation("release revision is required"))?,
                            input,
                            policy,
                            run,
                        )?;
                    run["result"] = serde_json::to_value(store.pool_task(ctx, id)?)
                        .map_err(|_| PlatformError::Storage)?;
                    publication
                }
                WorkflowTransitionStep::Handoff { workflow_id } => {
                    crate::task_handoff_provider_evidence::TaskHandoffProviderEvidence::refresh(
                        store,
                        ctx,
                        project,
                        input,
                        policy,
                        &run["outcomes"],
                    )?;
                    let current: WorkTask = PlatformStore::decode(run["result"].clone())?;
                    let result = crate::task_stage_store::TaskStageStore::handoff(
                        store,
                        ctx,
                        id,
                        current.revision,
                        workflow_id,
                        &run["outcomes"],
                        run,
                        index,
                    )?;
                    run["result"] = json!(result);
                    json!({"workflow_id":workflow_id,"task_revision":result.revision})
                }
                WorkflowTransitionStep::AdvanceState => {
                    // Re-read all provider facts immediately before the atomic state transaction.
                    for prior in steps.iter().take(index) {
                        match prior {
                            WorkflowTransitionStep::VerifyPush { repository_id } => {
                                for branch in Self::branches(input, repository_id.as_deref())? {
                                    Self::verify_branch(
                                        store, ctx, project, id, branch, None, policy, run,
                                    )?;
                                }
                            }
                            WorkflowTransitionStep::CreatePullRequest {
                                repository_id,
                                source_branch,
                                target_branch,
                                source_from_task,
                                target_from_parent,
                            } => {
                                let prior_index = steps
                                    .iter()
                                    .position(|step| std::ptr::eq(step, prior))
                                    .ok_or(PlatformError::Storage)?;
                                Self::create_requests(
                                    store,
                                    ctx,
                                    project,
                                    &task,
                                    input,
                                    repository_id.as_deref(),
                                    source_branch,
                                    target_branch,
                                    *source_from_task,
                                    *target_from_parent,
                                    prior_index,
                                    true,
                                    policy,
                                    run,
                                )?;
                            }
                            WorkflowTransitionStep::VerifyPipeline {
                                repository_id,
                                workflow,
                            } => {
                                let prior_index = steps
                                    .iter()
                                    .position(|step| std::ptr::eq(step, prior))
                                    .ok_or(PlatformError::Storage)?;
                                for branch in Self::branches(input, repository_id.as_deref())? {
                                    Self::pipeline(
                                        store,
                                        ctx,
                                        project,
                                        branch,
                                        workflow.as_deref(),
                                        prior_index,
                                        policy,
                                        run,
                                    )?;
                                }
                            }
                            WorkflowTransitionStep::VerifyPullRequestMerged {
                                repository_id,
                                target_branch,
                                target_from_parent,
                            } => {
                                for branch in Self::branches(input, repository_id.as_deref())? {
                                    let target =
                                        crate::task_branch_selectors::TaskBranchSelectors::target(
                                            store,
                                            &task,
                                            &branch.repository_id,
                                            target_branch.as_deref(),
                                            *target_from_parent,
                                        )?;
                                    TaskTransitionProvider::observe_to(
                                        store,
                                        ctx,
                                        project,
                                        branch,
                                        "pull_request_merged",
                                        None,
                                        target.as_deref(),
                                        policy,
                                    )?;
                                }
                            }
                            WorkflowTransitionStep::MergePullRequest {
                                repository_id,
                                target_branch,
                                target_from_parent,
                            } => {
                                for branch in Self::branches(input, repository_id.as_deref())? {
                                    let target =
                                        crate::task_branch_selectors::TaskBranchSelectors::target(
                                            store,
                                            &task,
                                            &branch.repository_id,
                                            Some(target_branch),
                                            *target_from_parent,
                                        )?;
                                    TaskTransitionProvider::observe_to(
                                        store,
                                        ctx,
                                        project,
                                        branch,
                                        "pull_request_merged",
                                        None,
                                        target.as_deref(),
                                        policy,
                                    )?;
                                }
                            }
                            WorkflowTransitionStep::VerifyDeployment {
                                repository_id,
                                environment,
                                target_branch,
                                target_from_parent,
                            } => {
                                for branch in Self::branches(input, repository_id.as_deref())? {
                                    let target =
                                        crate::task_branch_selectors::TaskBranchSelectors::target(
                                            store,
                                            &task,
                                            &branch.repository_id,
                                            target_branch.as_deref(),
                                            *target_from_parent,
                                        )?;
                                    TaskTransitionProvider::observe_to(
                                        store,
                                        ctx,
                                        project,
                                        branch,
                                        "deployment",
                                        Some(environment),
                                        target.as_deref(),
                                        policy,
                                    )?;
                                }
                            }
                            WorkflowTransitionStep::GenerateContributorRatings {
                                repository_id,
                            } => {
                                for branch in Self::branches(input, repository_id.as_deref())? {
                                    crate::task_contributor_rating::TaskContributorRating::context(
                                        store,
                                        id,
                                        task.revision,
                                        &branch.repository_id,
                                        &branch.expected_head_sha,
                                    )?;
                                }
                            }
                            WorkflowTransitionStep::CreateCodeReview { repository_id } => {
                                let prior_index = steps
                                    .iter()
                                    .position(|step| std::ptr::eq(step, prior))
                                    .ok_or(PlatformError::Storage)?;
                                for branch in Self::branches(input, repository_id.as_deref())? {
                                    let review = run["outcomes"]
                                        .as_array()
                                        .and_then(|outcomes| {
                                            outcomes.iter().find(|o| {
                                                o["step_index"].as_u64() == Some(prior_index as u64)
                                            })
                                        })
                                        .and_then(|o| o["result"]["reviews"].as_array())
                                        .and_then(|reviews| {
                                            reviews.iter().find(|r| {
                                                r["repository_id"] == branch.repository_id
                                            })
                                        })
                                        .ok_or(PlatformError::Storage)?;
                                    crate::task_pre_pr_review::TaskPrePrReview::context(
                                        store,
                                        ctx,
                                        project.ok_or(PlatformError::Forbidden)?,
                                        id,
                                        task.revision,
                                        &branch.repository_id,
                                        &branch.expected_head_sha,
                                        review["id"].as_str().ok_or(PlatformError::Storage)?,
                                    )?;
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(base) = required_base {
                        for branch in Self::branches(input, None)? {
                            Self::verify_branch(
                                store,
                                ctx,
                                project,
                                id,
                                branch,
                                Some(base),
                                policy,
                                run,
                            )?;
                        }
                    }
                    if flow
                        .graph
                        .states
                        .iter()
                        .any(|state| state.id == edge.to && state.terminal)
                    {
                        crate::task_subtask_integration::TaskSubtaskIntegration::verify(
                            store, ctx, &task, policy,
                        )?;
                    }
                    // Advance and durable run checkpoint share one transaction via the core transition store.
                    store.transition_run_checkpoint = Some((
                        run["id"].as_str().ok_or(PlatformError::Storage)?.to_owned(),
                        index + 1,
                    ));
                    let outer = store.request_context.take();
                    let result =
                        TaskTransitionStore::advance(store, ctx, project, id, transition, input);
                    store.request_context = outer;
                    store.transition_run_checkpoint = None;
                    let task = result?;
                    run["advanced"] = json!(true);
                    run["result"] = json!(task);
                    json!({"task_revision":task.revision})
                }
                WorkflowTransitionStep::Notify { channel, audience } => Self::notify(
                    store,
                    ctx,
                    id,
                    index,
                    channel,
                    audience.as_deref().unwrap_or("assignee"),
                    run,
                )?,
            };
            run["outcomes"].as_array_mut().ok_or(PlatformError::Storage)?.push(json!({"step_index":index,"kind":serde_json::to_value(step).map_err(|_|PlatformError::Storage)?["kind"],"result":outcome}));
            run["next_step"] = json!(index + 1);
            TaskTransitionRunStore::save(store, run)?;
        }
        run["status"] = json!("succeeded");
        TaskTransitionRunStore::save(store, run)?;
        PlatformStore::decode(run["result"].clone())
    }
    #[allow(clippy::too_many_arguments)]
    fn create_requests(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        task: &WorkTask,
        input: &TaskTransitionRequest,
        repository: Option<&str>,
        literal_source: &str,
        literal_target: &str,
        from_task: bool,
        from_parent: bool,
        index: usize,
        revalidate: bool,
        policy: &GitNetworkPolicy,
        run: &mut Value,
    ) -> Result<Vec<Value>, PlatformError> {
        let mut results = Vec::new();
        for branch in Self::branches(input, repository)? {
            let Some(source) = crate::task_branch_selectors::TaskBranchSelectors::source(
                store,
                task,
                branch,
                literal_source,
                from_task,
                input.release_id.is_some(),
            )?
            else {
                continue;
            };
            let target = crate::task_branch_selectors::TaskBranchSelectors::target(
                store,
                task,
                &branch.repository_id,
                Some(literal_target),
                from_parent,
            )?
            .ok_or(PlatformError::Storage)?;
            if revalidate && Self::merged_branch(run, branch, Some(&target)).is_some() {
                results.push(TaskTransitionProvider::observe_to(
                    store,
                    ctx,
                    project,
                    branch,
                    "pull_request_merged",
                    None,
                    Some(&target),
                    policy,
                )?);
            } else {
                results.push(TaskTransitionProvider::create(
                    store,
                    ctx,
                    project,
                    branch,
                    &source,
                    &target,
                    &task.title,
                    index,
                    policy,
                    run,
                )?);
            }
        }
        if results.is_empty() {
            return Err(PlatformError::Conflict(
                "pull request source requires an exact branch fact matching its configured literal or registered task branch",
            ));
        }
        Ok(results)
    }
    fn merged_branch(
        run: &Value,
        branch: &TaskTransitionBranch,
        target: Option<&str>,
    ) -> Option<Value> {
        run["outcomes"]
            .as_array()?
            .iter()
            .rev()
            .filter(|step| {
                step["kind"] == "merge_pull_request" || step["kind"] == "verify_pull_request_merged"
            })
            .filter_map(|step| step["result"]["pull_requests"].as_array())
            .flatten()
            .find(|fact| {
                fact["repository_id"] == branch.repository_id
                    && fact["head_sha"] == branch.expected_head_sha
                    && fact["status"] == "merged"
                    && target.is_none_or(|t| fact["target_branch"] == t)
            })
            .cloned()
    }
    fn merged_before(run: &Value, branch: &TaskTransitionBranch, index: usize) -> Option<Value> {
        let outcomes: Vec<_> = run["outcomes"]
            .as_array()?
            .iter()
            .filter(|step| {
                step["step_index"]
                    .as_u64()
                    .is_some_and(|i| i < index as u64)
            })
            .cloned()
            .collect();
        Self::merged_branch(&json!({"outcomes":outcomes}), branch, None)
    }
    #[allow(clippy::too_many_arguments)]
    fn pipeline(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        branch: &TaskTransitionBranch,
        workflow: Option<&str>,
        index: usize,
        policy: &GitNetworkPolicy,
        run: &Value,
    ) -> Result<Value, PlatformError> {
        if let Some(merged) = Self::merged_before(run, branch, index) {
            TaskTransitionProvider::observe_to(
                store,
                ctx,
                project,
                branch,
                "merged_pipeline",
                workflow,
                merged["target_branch"].as_str(),
                policy,
            )
        } else {
            let kind = if Self::merged_branch(run, branch, None).is_some() {
                "commit_pipeline"
            } else {
                "pipeline"
            };
            TaskTransitionProvider::observe(store, ctx, project, branch, kind, workflow, policy)
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn verify_branch(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        task: &str,
        branch: &TaskTransitionBranch,
        base: Option<&str>,
        policy: &GitNetworkPolicy,
        run: &mut Value,
    ) -> Result<Value, PlatformError> {
        if let Some(merged) = Self::merged_branch(run, branch, None) {
            return TaskTransitionProvider::observe_to(
                store,
                ctx,
                project,
                branch,
                "pull_request_merged",
                None,
                merged["target_branch"].as_str(),
                policy,
            );
        }
        let key = format!("{}:{}", branch.repository_id, branch.branch_name);
        let pinned = run["branch_bases"][&key].as_str().map(str::to_owned);
        let origin = if pinned.is_none() {
            store.client.query_opt("SELECT b.base_sha FROM task_repository_branches b JOIN task_subtasks s ON s.child_task_id=b.task_id AND s.removed_at IS NULL WHERE b.task_id=$1 AND b.repository_id=$2 AND b.branch_name=$3",&[&task,&branch.repository_id,&branch.branch_name]).map_err(PlatformStore::database_error)?.map(|row|row.get::<_,String>(0))
        } else {
            None
        };
        let result = TaskTransitionProvider::verify(
            store,
            ctx,
            project,
            branch,
            pinned.as_deref().or(origin.as_deref()).or(base),
            policy,
        )?;
        if !run["branch_bases"].is_object() {
            run["branch_bases"] = json!({});
        }
        run["branch_bases"][&key] = result["base_sha"].clone();
        let mut registered_evidence = result.clone();
        registered_evidence["workflow_version_id"] = run["workflow_version_id"].clone();
        registered_evidence["task_revision"] = run["task_revision"].clone();
        store.client.execute("UPDATE task_repository_branches SET head_sha=$4,evidence=$5 WHERE task_id=$1 AND repository_id=$2 AND branch_name=$3",&[&task,&branch.repository_id,&branch.branch_name,&branch.expected_head_sha,&registered_evidence]).map_err(PlatformStore::database_error)?;
        TaskTransitionRunStore::save(store, run)?;
        Ok(result)
    }
    fn request_approval(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        id: &str,
        transition: &str,
        input: &TaskTransitionRequest,
        run: &mut Value,
    ) -> Result<(), PlatformError> {
        if run["approval_request_id"].is_string() {
            return Ok(());
        }
        if let Some(project) = project {
            let mut parameters = input.clone();
            parameters.approval_id = None;
            let request = ApprovalRequestCreate {
                target: ApprovalRequestTarget::Task {
                    task_id: id.into(),
                    transition_id: transition.into(),
                    parameters,
                },
                reason: input.reason.clone().unwrap_or_else(|| {
                    format!("Approval required for task transition {transition}")
                }),
                expires_in_seconds: 86400,
            };
            let key = format!(
                "{}:approval",
                run["id"].as_str().ok_or(PlatformError::Storage)?
            );
            let outer = store.request_context.take();
            let created=store.request(ctx,"approval_request.create",&key,&json!({"project_id":project,"request":request}),|store|crate::approval_request_definition_store::ApprovalRequestDefinitionStore::create(store,ctx,project,&request));
            store.request_context = outer;
            let request = created?;
            run["approval_request_id"] = json!(request.id);
            TaskTransitionRunStore::save(store, run)?;
        } else {
            let key = format!(
                "{}:approval",
                run["id"].as_str().ok_or(PlatformError::Storage)?
            );
            store.client.execute("INSERT INTO in_app_notifications(id,recipient_id,kind,resource_id) SELECT $1||':'||u.actor_id,u.actor_id,'task_updated',$2 FROM users u JOIN actors a ON a.id=u.actor_id AND a.active WHERE u.actor_id<>$3 AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=u.actor_id AND g.project_id IS NULL AND g.capability='approvals:grant') ON CONFLICT DO NOTHING",&[&key,&id,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        }
        Ok(())
    }
    fn branches<'a>(
        input: &'a TaskTransitionRequest,
        repository: Option<&str>,
    ) -> Result<Vec<&'a TaskTransitionBranch>, PlatformError> {
        if input.branches.is_empty() || input.branches.len() > 8 {
            return Err(PlatformError::Conflict(
                "provide 1..8 repository branch facts including expected_head_sha",
            ));
        }
        let mut ids = std::collections::BTreeSet::new();
        for branch in &input.branches {
            if !ids.insert((&branch.repository_id, &branch.branch_name))
                || !DevelopmentPolicy::valid_ref(&branch.branch_name)
                || !TaskWorkBranchReport::valid_sha(&branch.expected_head_sha)
            {
                return Err(PlatformError::Validation(
                    "invalid or duplicate transition branch facts",
                ));
            }
        }
        let branches: Vec<_> = input
            .branches
            .iter()
            .filter(|branch| repository.is_none_or(|id| id == branch.repository_id))
            .collect();
        if branches.is_empty() {
            return Err(PlatformError::Conflict(
                "transition step repository requires an expected branch fact",
            ));
        }
        Ok(branches)
    }
    fn preflight(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        id: &str,
        transition: &str,
        input: &TaskTransitionRequest,
        flow: &WorkflowVersion,
    ) -> Result<(), PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let task = crate::task_policy_store::TaskPolicyStore::scoped_task(&mut tx, project, id)?;
        if tx.query_one("SELECT EXISTS(SELECT 1 FROM task_transition_runs WHERE task_id=$1 AND task_revision=$2 AND record->>'execution_kind'='take_branches' AND record->>'status'<>'succeeded')",&[&id,&task.revision]).map_err(PlatformStore::database_error)?.get::<_,bool>(0){
            return Err(PlatformError::Conflict("repository work preparation is incomplete; retry Take with the same repository selection before transitioning this stage"));
        }
        if task.governed_stage && task.assignee_id.is_none() {
            return Err(PlatformError::Conflict(
                "take the current workflow stage before transitioning",
            ));
        }
        if task.archived || task.revision != input.expected_revision {
            return Err(PlatformError::Conflict(
                "task revision changed or task archived",
            ));
        }
        if matches!(
            task.lifecycle,
            TaskLifecycle::Pending | TaskLifecycle::Paused
        ) {
            return Err(PlatformError::Conflict(
                "task must be taken and resumed before transitioning",
            ));
        }
        let mut evaluation = crate::task_transition_evaluation::TaskTransitionEvaluation::build(
            &mut tx, &task, input, flow, transition,
        )?;
        evaluation.working_branch_reported =
            crate::task_work_store::TaskWorkStore::condition(&mut tx, ctx, &task, transition)?;
        let definition = flow
            .graph
            .transitions
            .iter()
            .find(|edge| edge.id == transition)
            .ok_or(PlatformError::Validation("transition is outside workflow"))?;
        evaluation.human_approval_valid =
            if definition.guards.contains(&WorkflowGuard::HumanApproval) {
                crate::task_approval_store::TaskApprovalStore::consume(
                    &mut tx, ctx, &task, flow, transition, input,
                )?
            } else {
                if !definition.requires_approval() && input.approval_id.is_some() {
                    return Err(PlatformError::Validation(
                        "transition does not accept an approval",
                    ));
                }
                // Ordered approval is evaluated only when its step is reached.
                true
            };
        let edge = flow
            .graph
            .transition(task.execution_state()?, transition, &evaluation)?;
        let terminal = flow
            .graph
            .states
            .iter()
            .any(|state| state.id == edge.to && state.terminal);
        crate::task_relation_policy::TaskRelationPolicy::state(&mut tx, &task, terminal)?;
        // Validation is read-only in effect: approval is consumed only with atomic advancement.
        tx.rollback().map_err(PlatformStore::database_error)
    }
    fn notify(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        id: &str,
        index: usize,
        channel: &str,
        audience: &str,
        run: &Value,
    ) -> Result<Value, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let task = crate::task_scope_authority::TaskScopeAuthority::task(
            &mut tx,
            ctx,
            id,
            Capability::TasksTransition,
        )?;
        let key = format!(
            "workflow:{}:{index}",
            run["id"].as_str().ok_or(PlatformError::Storage)?
        );
        if audience == "approvers" {
            if channel == "email" {
                tx.execute("UPDATE email_notifications SET status='pending',attempts=0,available_at=clock_timestamp() WHERE strpos(event_key,$1||':')=1 AND status='failed'",&[&key]).map_err(PlatformStore::database_error)?;
                tx.execute("INSERT INTO email_notifications(event_key,kind,project_id,resource_id,actor_id) SELECT $1||':'||r.actor_id,'workflow_transition',$2,$3,r.actor_id FROM notification_recipients r JOIN actors a ON a.id=r.actor_id AND a.active AND a.kind='human' WHERE r.actor_id<>$4 AND r.email<>'' AND r.approvals AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=r.actor_id AND g.capability='approvals:grant' AND (g.project_id IS NULL OR g.project_id=$2)) AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=r.actor_id AND g.capability='tasks:read' AND (g.project_id IS NULL OR g.project_id=$2)) AND EXISTS(SELECT 1 FROM actor_roles role WHERE role.actor_id=r.actor_id AND (role.role='super_admin' OR role.role IN ('technical_lead','pmo','manager') AND ($2::text IS NULL OR role.all_projects OR EXISTS(SELECT 1 FROM member_role_projects scope WHERE scope.actor_id=role.actor_id AND scope.project_id=$2)))) ON CONFLICT DO NOTHING",&[&key,&task.project_id,&id,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
            } else {
                tx.execute("INSERT INTO in_app_notifications(id,recipient_id,kind,project_id,resource_id) SELECT $1||':'||u.actor_id,u.actor_id,'task_updated',$2,$3 FROM users u JOIN actors a ON a.id=u.actor_id AND a.active AND a.kind='human' WHERE u.actor_id<>$4 AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=u.actor_id AND g.capability='approvals:grant' AND (g.project_id IS NULL OR g.project_id=$2)) AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=u.actor_id AND g.capability='tasks:read' AND (g.project_id IS NULL OR g.project_id=$2)) AND EXISTS(SELECT 1 FROM actor_roles role WHERE role.actor_id=u.actor_id AND (role.role='super_admin' OR role.role IN ('technical_lead','pmo','manager') AND ($2::text IS NULL OR role.all_projects OR EXISTS(SELECT 1 FROM member_role_projects scope WHERE scope.actor_id=role.actor_id AND scope.project_id=$2)))) ON CONFLICT DO NOTHING",&[&key,&task.project_id,&id,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
            }
        } else if let Some(actor) = &task.assignee_id {
            if channel == "email" {
                tx.execute("UPDATE email_notifications SET status='pending',attempts=0,available_at=clock_timestamp() WHERE event_key=$1 AND status='failed'",&[&key]).map_err(PlatformStore::database_error)?;
                tx.execute("INSERT INTO email_notifications(event_key,kind,project_id,resource_id,actor_id) SELECT $1,'workflow_transition',$2,$3,$4 FROM notification_recipients r JOIN actors a ON a.id=r.actor_id AND a.active WHERE r.actor_id=$4 AND r.email<>'' AND r.assignments ON CONFLICT DO NOTHING",&[&key,&task.project_id,&id,&actor]).map_err(PlatformStore::database_error)?;
            } else {
                tx.execute("INSERT INTO in_app_notifications(id,recipient_id,kind,project_id,resource_id) SELECT $1,$2,'task_updated',$3,$4 FROM users u JOIN actors a ON a.id=u.actor_id AND a.active WHERE u.actor_id=$2 ON CONFLICT DO NOTHING",&[&key,&actor,&task.project_id,&id]).map_err(PlatformStore::database_error)?;
            }
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(
            json!({"channel":channel,"audience":audience,"status":"queued_or_skipped_by_recipient_preferences","event_key":key}),
        )
    }
}

#[cfg(test)]
#[path = "task_transition_engine_tests.rs"]
mod tests;
