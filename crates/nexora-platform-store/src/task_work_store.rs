use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    development_policy_store::DevelopmentPolicyStore, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter, repository_access::RepositoryAccess,
    task_policy_store::TaskPolicyStore,
};
use nexora_platform::*;
use serde_json::json;
pub(crate) struct TaskWorkStore;
impl TaskWorkStore {
    pub fn session(
        tx: &mut postgres::Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        task: &str,
        id: &str,
    ) -> Result<TaskWorkSession, PlatformError> {
        let row=tx.query_opt("SELECT record,task_revision,workflow_version_id,transition_id,reported FROM task_work_sessions WHERE id=$1 AND project_id=$2 AND task_id=$3 AND actor_id=$4 FOR UPDATE",&[&id,&project,&task,&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        let session: TaskWorkSession = PlatformStore::decode(row.get(0))?;
        if session.id != id
            || session.project_id != project
            || session.task_id != task
            || session.actor_id != ctx.actor_id()
            || session.task_revision != row.get::<_, i64>(1)
            || session.workflow_version_id != row.get::<_, String>(2)
            || session.transition_id != row.get::<_, String>(3)
            || session.report.is_some() != row.get::<_, bool>(4)
            || session.branches.is_empty()
        {
            return Err(PlatformError::Storage);
        }
        Ok(session)
    }
    pub fn source_authority(
        tx: &mut postgres::Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
    ) -> Result<serde_json::Value, PlatformError> {
        let source = crate::repository_snapshot_scope::RepositorySnapshotScope::lock(
            tx, ctx, project, repository, None,
        )?;
        let mut authority = source.authority(tx)?;
        let row=tx.query_one("SELECT host,remote_full_name FROM project_repositories WHERE project_id=$1 AND id=$2",&[&project,&repository]).map_err(PlatformStore::database_error)?;
        authority["host"] = json!(row.get::<_, Option<String>>(0));
        authority["remote_full_name"] = json!(row.get::<_, Option<String>>(1));
        Ok(authority)
    }
    pub fn policies(
        tx: &mut postgres::Transaction<'_>,
        ctx: &AuthContext,
        session: &TaskWorkSession,
    ) -> Result<(), PlatformError> {
        let flow = crate::task_pool_workflow::TaskPoolWorkflow::flow(
            tx,
            Some(&session.project_id),
            &session.workflow_version_id,
            false,
        )?;
        let state_base = flow
            .graph
            .transitions
            .iter()
            .find(|edge| edge.id == session.transition_id)
            .and_then(|edge| flow.graph.states.iter().find(|state| state.id == edge.from))
            .and_then(|state| state.required_base_branch.as_deref());
        let governed: bool = tx
            .query_one(
                "SELECT governed_stage FROM tasks WHERE id=$1 AND project_id=$2",
                &[&session.task_id, &session.project_id],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        for branch in &session.branches {
            if Self::source_authority(tx, ctx, &session.project_id, &branch.repository_id)?
                != branch.source_authority
            {
                return Err(PlatformError::Conflict("repository work authority changed"));
            }
            RepositoryAccess::lock(tx, ctx, &session.project_id, &branch.repository_id)?;
            let policy =
                DevelopmentPolicyStore::lock(tx, &session.project_id, &branch.repository_id)?;
            let origin_matches = if governed {
                tx.query_opt("SELECT branch_name,base_branch,base_sha FROM task_repository_branches WHERE task_id=$1 AND repository_id=$2",&[&session.task_id,&branch.repository_id]).map_err(PlatformStore::database_error)?.is_some_and(|row|row.get::<_,String>(0)==branch.branch_name&&row.get::<_,String>(1)==branch.base_branch&&Some(row.get::<_,String>(2)).as_ref()==branch.base_sha.as_ref())
            } else {
                state_base.unwrap_or(&policy.base_branch) == branch.base_branch
            };
            if policy.revision != branch.policy_revision
                || !origin_matches
                || policy.branch_prefix != branch.branch_prefix
            {
                return Err(PlatformError::Conflict("development policy changed"));
            }
        }
        Ok(())
    }
    pub fn condition(
        tx: &mut postgres::Transaction<'_>,
        ctx: &AuthContext,
        task: &WorkTask,
        transition: &str,
    ) -> Result<bool, PlatformError> {
        let id:Option<String>=tx.query_opt("SELECT id FROM task_work_sessions WHERE project_id=$1 AND task_id=$2 AND actor_id=$3 AND task_revision=$4 AND workflow_version_id=$5 AND transition_id=$6 AND reported ORDER BY created_at DESC,id DESC LIMIT 1 FOR SHARE",&[&task.project_id,&task.id,&ctx.actor_id(),&task.revision,&task.workflow_version_id,&transition]).map_err(PlatformStore::database_error)?.map(|r|r.get(0));
        let Some(id) = id else { return Ok(false) };
        let session = Self::session(tx, ctx, task.project_scope()?, &task.id, &id)?;
        Self::policies(tx, ctx, &session)?;
        Ok(session.branches.iter().all(|b| {
            b.base_sha.as_ref().is_some_and(|s| {
                matches!(s.len(), 40 | 64) && s.bytes().all(|c| c.is_ascii_hexdigit())
            }) && b
                .head_sha
                .as_ref()
                .is_some_and(|s| TaskWorkBranchReport::valid_sha(s))
        }))
    }
}
impl PlatformStore {
    pub fn prepare_task_work(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        task: &str,
        input: &TaskWorkPrepare,
    ) -> Result<TaskWorkSession, PlatformError> {
        if input.repository_ids.is_empty()
            || input.repository_ids.len() > 8
            || input.parameters.expected_revision < 1
            || input.transition_id.is_empty()
            || input.transition_id.len() > 128
        {
            return Err(PlatformError::Validation("invalid task work preparation"));
        }
        let mut ids = input.repository_ids.clone();
        ids.sort();
        ids.dedup();
        if ids.len() != input.repository_ids.len() {
            return Err(PlatformError::Validation("duplicate repository"));
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        for cap in [
            Capability::TasksRead,
            Capability::TasksWrite,
            Capability::TasksTransition,
        ] {
            PlatformAuthority::require(&mut tx, ctx, cap, Some(project))?;
        }
        PlatformAuthority::project(&mut tx, project)?;
        let before = TaskPolicyStore::task(&mut tx, project, task)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.work.prepare",
        )? {
            let session: TaskWorkSession = Self::decode(value)?;
            TaskWorkStore::policies(&mut tx, ctx, &session)?;
            tx.commit().map_err(Self::database_error)?;
            return Ok(session);
        }
        if before.archived || before.revision != input.parameters.expected_revision {
            return Err(PlatformError::Conflict(
                "task revision changed or task archived",
            ));
        }
        let flow = TaskPolicyStore::flow(&mut tx, project, before.execution_version()?, false)?;
        let edge = flow
            .graph
            .transitions
            .iter()
            .find(|e| {
                e.id == input.transition_id && before.state_id.as_deref() == Some(e.from.as_str())
            })
            .ok_or(PlatformError::Validation(
                "transition does not start from task state",
            ))?;
        if !edge.guards.contains(&WorkflowGuard::WorkingBranchReported) {
            return Err(PlatformError::Validation(
                "start transition must require working_branch_reported",
            ));
        }
        let id = SecretGenerator::id("wrk")?;
        let mut branches = Vec::new();
        for repository in ids {
            RepositoryAccess::lock(&mut tx, ctx, project, &repository)?;
            let source_authority =
                TaskWorkStore::source_authority(&mut tx, ctx, project, &repository)?;
            let policy = DevelopmentPolicyStore::lock(&mut tx, project, &repository)?;
            let governed_branch = if before.governed_stage {
                Some(tx.query_opt("SELECT branch_name,base_branch,base_sha,head_sha,evidence FROM task_repository_branches WHERE task_id=$1 AND repository_id=$2",&[&before.id,&repository]).map_err(Self::database_error)?.ok_or(PlatformError::Conflict("take implementation work with selected repositories before preparing its provider branch"))?)
            } else {
                None
            };
            let branch_name = if let Some(row) = &governed_branch {
                row.get::<_, String>(0)
            } else {
                format!(
                    "{}/{}-{}",
                    policy.branch_prefix,
                    before.id,
                    &id[id.len() - 8..]
                )
            };
            if !DevelopmentPolicy::valid_ref(&branch_name) {
                return Err(PlatformError::Validation(
                    "prepared branch exceeds Git bounds",
                ));
            }
            let base_branch = if let Some(row) = &governed_branch {
                row.get::<_, String>(1)
            } else {
                flow.graph
                    .states
                    .iter()
                    .find(|state| before.state_id.as_deref() == Some(state.id.as_str()))
                    .and_then(|state| state.required_base_branch.clone())
                    .unwrap_or(policy.base_branch)
            };
            if let Some(row) = &governed_branch {
                let evidence: serde_json::Value = row.get(4);
                if evidence["workflow_version_id"]
                    .as_str()
                    .is_some_and(|version| Some(version) != before.workflow_version_id.as_deref())
                {
                    return Err(PlatformError::Conflict(
                        "registered provider branch belongs to an earlier workflow stage",
                    ));
                }
            }
            branches.push(TaskWorkBranch {
                repository_id: repository,
                base_branch,
                branch_prefix: policy.branch_prefix,
                branch_name,
                policy_revision: policy.revision,
                source_authority,
                base_sha: governed_branch.as_ref().map(|row| row.get(2)),
                head_sha: governed_branch.as_ref().map(|row| row.get(3)),
            });
        }
        let session = TaskWorkSession {
            id,
            project_id: project.into(),
            task_id: task.into(),
            actor_id: ctx.actor_id().into(),
            task_revision: before.revision,
            workflow_version_id: before.execution_version()?.to_owned(),
            transition_id: input.transition_id.clone(),
            parameters: input.parameters.clone(),
            branches,
            report: None,
        };
        let value = serde_json::to_value(&session).map_err(|_| PlatformError::Storage)?;
        tx.execute("INSERT INTO task_work_sessions(id,project_id,task_id,actor_id,task_revision,workflow_version_id,transition_id,record) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",&[&session.id,&project,&task,&ctx.actor_id(),&session.task_revision,&session.workflow_version_id,&session.transition_id,&value]).map_err(Self::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "task.work.prepared",
            Some(task),
            json!({"session_id":session.id,"repositories":session.branches.iter().map(|b|&b.repository_id).collect::<Vec<_>>()}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(session)
    }
    pub fn task_work(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        task: &str,
        id: &str,
    ) -> Result<TaskWorkSession, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::TasksRead, Some(project))?;
        PlatformAuthority::project(&mut tx, project)?;
        crate::task_read_authority::TaskReadAuthority::require_task(&mut tx, ctx, project, task)?;
        TaskPolicyStore::task(&mut tx, project, task)?;
        let session = TaskWorkStore::session(&mut tx, ctx, project, task, id)?;
        for branch in &session.branches {
            TaskWorkStore::source_authority(&mut tx, ctx, project, &branch.repository_id)?;
        }
        tx.commit().map_err(Self::database_error)?;
        Ok(session)
    }
}
