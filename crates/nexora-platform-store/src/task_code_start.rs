use crate::{
    GitNetworkPolicy, PlatformStore, SecretGenerator, receipt_writer::ReceiptWriter,
    task_transition_provider::TaskTransitionProvider,
    task_transition_run_store::TaskTransitionRunStore,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct TaskCodeStart;
impl TaskCodeStart {
    pub fn execute(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        id: &str,
        input: &TaskCodeTake,
        project: Option<&str>,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        let before = store.pool_task(ctx, id)?;
        if project.is_some() && before.project_id.as_deref() != project {
            return Err(PlatformError::NotFound);
        }
        let needs_repositories = if before.governed_stage
            && before.purpose == TaskPurpose::Implementation
        {
            if let Some(project) = &before.project_id {
                store.client.query_one("SELECT EXISTS(SELECT 1 FROM project_repositories WHERE project_id=$1 AND NOT archived) OR EXISTS(SELECT 1 FROM workflow_versions v JOIN workflows w ON w.id=v.workflow_id WHERE w.id=COALESCE($2,$3) AND v.id=COALESCE($4,w.latest_version_id) AND EXISTS(SELECT 1 FROM jsonb_array_elements(v.graph->'states') state WHERE (state->>'initial')::boolean IS TRUE AND state->>'required_base_branch' IS NOT NULL))",&[&project,&before.workflow_id,&before.entry_workflow_id,&before.workflow_version_id]).map_err(PlatformStore::database_error)?.get::<_,bool>(0)
            } else {
                false
            }
        } else {
            false
        };
        if input.repository_ids.is_empty() {
            if needs_repositories {
                return Err(PlatformError::Validation(
                    "select the repositories for implementation work before taking this stage",
                ));
            }
            return serde_json::to_value(store.take_task(ctx, id, &input.take(), project)?)
                .map_err(|_| PlatformError::Storage);
        }
        if before.governed_stage && before.purpose != TaskPurpose::Implementation {
            return Err(PlatformError::Validation(
                "repository branches are selected only for implementation work",
            ));
        }
        if before.project_id.is_none() {
            return Err(PlatformError::Validation(
                "repository branches require a project-linked task",
            ));
        }
        if input.repository_ids.len() > 8 {
            return Err(PlatformError::Validation(
                "select at most eight repositories for a single task operation",
            ));
        }
        let mut ids = input.repository_ids.clone();
        ids.sort();
        ids.dedup();
        if ids.len() != input.repository_ids.len()
            || ids.iter().any(|id| id.is_empty() || id.len() > 128)
        {
            return Err(PlatformError::Validation(
                "invalid or duplicate repository selection",
            ));
        }
        let request = store
            .request_context
            .clone()
            .ok_or(PlatformError::Validation(
                "taking repository work requires an idempotency key",
            ))?;
        let lock = format!("task-transition:{id}");
        let receipt_lock = serde_json::to_string(&(
            &store.namespace,
            ctx.actor_id(),
            &request.operation,
            &request.request_id,
        ))
        .map_err(|_| PlatformError::Storage)?;
        if !store
            .client
            .query_one(
                "SELECT pg_try_advisory_lock(hashtextextended($1,0))",
                &[&lock],
            )
            .map_err(PlatformStore::database_error)?
            .get::<_, bool>(0)
        {
            return Err(PlatformError::Conflict(
                "task work is already starting or transitioning; retry its existing operation",
            ));
        }
        if let Err(error) = store
            .client
            .query_one(
                "SELECT pg_advisory_lock(hashtextextended($1,0))",
                &[&receipt_lock],
            )
            .map_err(PlatformStore::database_error)
        {
            store
                .client
                .query_one(
                    "SELECT pg_advisory_unlock(hashtextextended($1,0))",
                    &[&lock],
                )
                .map_err(PlatformStore::database_error)?;
            return Err(error);
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Self::locked(store, ctx, id, input, project, policy)
        }));
        store
            .client
            .query_one(
                "SELECT pg_advisory_unlock(hashtextextended($1,0))",
                &[&receipt_lock],
            )
            .map_err(PlatformStore::database_error)?;
        store
            .client
            .query_one(
                "SELECT pg_advisory_unlock(hashtextextended($1,0))",
                &[&lock],
            )
            .map_err(PlatformStore::database_error)?;
        match result {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
    fn locked(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        id: &str,
        input: &TaskCodeTake,
        project: Option<&str>,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.take",
        )?;
        let before = crate::task_scope_authority::TaskScopeAuthority::task(
            &mut tx,
            ctx,
            id,
            Capability::TasksTransition,
        )?;
        for repository in &input.repository_ids {
            let source = crate::repository_access::RepositoryAccess::lock(
                &mut tx,
                ctx,
                before.project_scope()?,
                repository,
            )?;
            if source.source_kind != RepositorySourceKind::Git {
                return Err(PlatformError::Validation(
                    "select authorized Git repositories for provider-created implementation branches",
                ));
            }
            crate::task_transition_provider_authority::TaskTransitionProviderAuthority::lock(
                &mut tx,
                ctx,
                before.project_scope()?,
                repository,
            )?;
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        let fingerprint = SecretGenerator::checksum(
            &serde_json::to_vec(input).map_err(|_| PlatformError::Storage)?,
        );
        let existing=store.client.query_opt("SELECT record FROM task_transition_runs WHERE task_id=$1 AND actor_id=$2 AND transition_id='__take_branches' AND fingerprint=$3 ORDER BY created_at DESC LIMIT 1",&[&id,&ctx.actor_id(),&fingerprint]).map_err(PlatformStore::database_error)?;
        let mut run = if let Some(row) = existing {
            row.get::<_, Value>(0)
        } else {
            let taken = store.take_task(ctx, id, &input.take(), project)?;
            let run = json!({"id":SecretGenerator::id("trn")?,"task_id":id,"actor_id":ctx.actor_id(),"task_revision":taken.revision,"transition_id":"__take_branches","execution_kind":"take_branches","workflow_version_id":taken.workflow_version_id,"status":"running","next_step":0,"outcomes":[],"plans":null,"provider_intents":{},"advanced":false,"result":taken});
            store.client.execute("INSERT INTO task_transition_runs(id,task_id,project_id,actor_id,credential_id,auth_method,task_revision,transition_id,fingerprint,parameters,record) VALUES($1,$2,$3,$4,$5,$6,$7,'__take_branches',$8,$9,$10)",&[&run["id"].as_str(),&id,&taken.project_id,&ctx.actor_id(),&ctx.credential_id(),&if ctx.method()==AuthMethod::ApiKey{"api_key"}else{"session"},&taken.revision,&fingerprint,&json!(input),&run]).map_err(PlatformStore::database_error)?;
            run
        };
        let task = TaskTransitionRunStore::authorize(
            store,
            ctx,
            project.or(before.project_id.as_deref()),
            id,
        )?;
        if Some(task.revision) != run["task_revision"].as_i64()
            || task.assignee_id.as_deref() != Some(ctx.actor_id())
        {
            return Err(PlatformError::Conflict(
                "task stage or assignee changed after repository work was started",
            ));
        }
        if run["status"] == "succeeded" {
            return Self::response(store, ctx, &run);
        }
        let result = Self::branches(store, ctx, &task, input, policy, &mut run);
        if let Err(error) = &result {
            TaskTransitionRunStore::fail(store, ctx, &mut run, error)?;
        }
        match result {
            Ok(()) => {}
            Err(
                PlatformError::Unavailable
                | PlatformError::NotFound
                | PlatformError::Conflict(_)
                | PlatformError::Validation(_),
            ) => {
                return Err(PlatformError::Conflict(
                    "task was taken but repository work preparation is incomplete; inspect its start run and retry Take with the same repository selection",
                ));
            }
            Err(error) => return Err(error),
        }
        run["status"] = json!("succeeded");
        TaskTransitionRunStore::save(store, &run)?;
        Self::response(store, ctx, &run)
    }
    fn branches(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        task: &WorkTask,
        input: &TaskCodeTake,
        policy: &GitNetworkPolicy,
        run: &mut Value,
    ) -> Result<(), PlatformError> {
        let project = task.project_scope()?;
        if !run["plans"].is_array() {
            let mut tx = store
                .client
                .transaction()
                .map_err(PlatformStore::database_error)?;
            let flow = crate::task_pool_workflow::TaskPoolWorkflow::flow(
                &mut tx,
                Some(project),
                task.execution_version()?,
                false,
            )?;
            let state_base = flow
                .graph
                .states
                .iter()
                .find(|state| task.state_id.as_deref() == Some(state.id.as_str()))
                .and_then(|state| state.required_base_branch.clone());
            tx.commit().map_err(PlatformStore::database_error)?;
            let mut plans = Vec::new();
            for repository in &input.repository_ids {
                let policy_record = store.development_policy(ctx, project, repository)?;
                let configured = state_base.as_deref().unwrap_or(&policy_record.base_branch);
                let (base, mut sha) = crate::task_branch_origin::TaskBranchOrigin::resolve(
                    store, &task.id, repository, configured, "",
                )?;
                if !TaskWorkBranchReport::valid_sha(&base) {
                    sha =
                        store.task_transition_branch_fact(
                            ctx, &task.id, repository, &base, policy,
                        )?["expected_head_sha"]
                            .as_str()
                            .ok_or(PlatformError::Unavailable)?
                            .to_owned();
                }
                let run_id = run["id"].as_str().ok_or(PlatformError::Storage)?;
                let name = format!(
                    "{}/{}-{}",
                    policy_record.branch_prefix,
                    task.id,
                    &run_id[run_id.len() - 8..]
                );
                if !DevelopmentPolicy::valid_ref(&name) {
                    return Err(PlatformError::Validation(
                        "generated task branch exceeds Git naming limits",
                    ));
                }
                let authority =
                    TaskTransitionProvider::authority(store, ctx, Some(project), repository)?;
                plans.push(json!({"repository_id":repository,"branch_name":name,"base_branch":base,"base_sha":sha,"authority":authority}));
            }
            run["plans"] = json!(plans);
            TaskTransitionRunStore::save(store, run)?;
        }
        let plans = run["plans"]
            .as_array()
            .ok_or(PlatformError::Storage)?
            .clone();
        let start = run["next_step"].as_u64().unwrap_or(0) as usize;
        for (index, plan) in plans.iter().enumerate().skip(start) {
            let branch = TaskTransitionBranch {
                repository_id: plan["repository_id"]
                    .as_str()
                    .ok_or(PlatformError::Storage)?
                    .into(),
                branch_name: plan["branch_name"]
                    .as_str()
                    .ok_or(PlatformError::Storage)?
                    .into(),
                expected_head_sha: plan["base_sha"]
                    .as_str()
                    .ok_or(PlatformError::Storage)?
                    .into(),
            };
            let mut planned_authority = plan["authority"].clone();
            let mut current_authority = TaskTransitionProvider::authority(
                store,
                ctx,
                Some(project),
                &branch.repository_id,
            )?;
            // Token renewal and content notifications do not alter repository identity or
            // policy; the exact base SHA stays pinned and live consent is rechecked below.
            for authority in [&mut planned_authority, &mut current_authority] {
                if let Some(fields) = authority.as_object_mut() {
                    fields.remove("credential_revision");
                    fields.remove("source_generation");
                }
            }
            if planned_authority != current_authority {
                return Err(PlatformError::Conflict(
                    "repository identity, consent or branch policy changed after work preparation",
                ));
            }
            let base = plan["base_branch"].as_str().ok_or(PlatformError::Storage)?;
            let mut evidence = TaskTransitionProvider::create_branch(
                store,
                ctx,
                Some(project),
                &branch,
                base,
                &branch.expected_head_sha,
                index,
                policy,
                run,
            )?;
            evidence["workflow_version_id"] = json!(task.workflow_version_id);
            evidence["task_revision"] = json!(task.revision);
            let verified_authority = store
                .transition_verified_facts
                .iter()
                .rev()
                .find(|fact| fact["repository_id"] == branch.repository_id)
                .cloned()
                .ok_or(PlatformError::Storage)?;
            let mut tx = store
                .client
                .transaction()
                .map_err(PlatformStore::database_error)?;
            crate::task_scope_authority::TaskScopeAuthority::task(
                &mut tx,
                ctx,
                &task.id,
                Capability::TasksTransition,
            )?;
            let current = crate::task_policy_store::TaskPolicyStore::scoped_task(
                &mut tx,
                Some(project),
                &task.id,
            )?;
            if current.revision != task.revision
                || current.workflow_id != task.workflow_id
                || current.workflow_version_id != task.workflow_version_id
                || current.state_id != task.state_id
                || current.lifecycle != TaskLifecycle::Active
                || current.assignee_id.as_deref() != Some(ctx.actor_id())
                || current.archived
            {
                return Err(PlatformError::Conflict(
                    "task stage, revision or assignee changed while its provider branch was created; reconcile the start run before continuing",
                ));
            }
            if crate::task_transition_provider_authority::TaskTransitionProviderAuthority::lock(
                &mut tx,
                ctx,
                project,
                &branch.repository_id,
            )? != verified_authority
            {
                return Err(PlatformError::Conflict(
                    "repository consent or branch policy changed after provider branch verification",
                ));
            }
            let previous:Option<Value>=tx.query_opt("SELECT to_jsonb(b) FROM task_repository_branches b WHERE task_id=$1 AND repository_id=$2 FOR UPDATE",&[&task.id,&branch.repository_id]).map_err(PlatformStore::database_error)?.map(|r|r.get(0));
            tx.execute("INSERT INTO task_repository_branches(task_id,repository_id,branch_name,base_branch,base_sha,head_sha,evidence) VALUES($1,$2,$3,$4,$5,$5,$6) ON CONFLICT(task_id,repository_id) DO UPDATE SET branch_name=EXCLUDED.branch_name,base_branch=EXCLUDED.base_branch,base_sha=EXCLUDED.base_sha,head_sha=EXCLUDED.head_sha,evidence=EXCLUDED.evidence",&[&task.id,&branch.repository_id,&branch.branch_name,&base,&branch.expected_head_sha,&evidence]).map_err(PlatformStore::database_error)?;
            crate::task_history_writer::TaskHistoryWriter::event(
                &mut tx,
                ctx,
                task,
                "task.repository_branch_created",
                json!({"run_id":run["id"],"branch":evidence,"previous_stage_branch":previous}),
            )?;
            let mut checkpoint = run.clone();
            checkpoint["outcomes"].as_array_mut().ok_or(PlatformError::Storage)?.push(json!({"step_index":index,"kind":"create_branch","result":{"branches":[evidence]}}));
            checkpoint["next_step"] = json!(index + 1);
            tx.execute("UPDATE task_transition_runs SET record=$2,updated_at=clock_timestamp() WHERE id=$1",&[&checkpoint["id"].as_str(),&checkpoint]).map_err(PlatformStore::database_error)?;
            tx.commit().map_err(PlatformStore::database_error)?;
            *run = checkpoint;
        }
        Ok(())
    }
    fn response(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        run: &Value,
    ) -> Result<Value, PlatformError> {
        let mut value = run["result"].clone();
        let branches: Vec<Value> = run["outcomes"]
            .as_array()
            .ok_or(PlatformError::Storage)?
            .iter()
            .filter_map(|outcome| outcome["result"]["branches"].as_array())
            .flatten()
            .cloned()
            .collect();
        value["repository_branches"] = json!(branches);
        value["start_run_id"] = run["id"].clone();
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        if ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.take",
        )?
        .is_none()
        {
            ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        }
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
}
impl PlatformStore {
    pub fn take_task_with_repositories(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &TaskCodeTake,
        project: Option<&str>,
        policy: &GitNetworkPolicy,
    ) -> Result<Value, PlatformError> {
        TaskCodeStart::execute(self, ctx, id, input, project, policy)
    }
}
