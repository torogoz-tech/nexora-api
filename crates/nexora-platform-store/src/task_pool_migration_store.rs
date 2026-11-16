use crate::{
    PlatformStore, receipt_writer::ReceiptWriter, task_history_writer::TaskHistoryWriter,
    task_pool_workflow::TaskPoolWorkflow, task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::*;
use serde_json::Value;
pub(crate) struct TaskPoolMigrationStore;
impl TaskPoolMigrationStore {
    pub fn migrate(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        id: &str,
        input: &TaskWorkflowMigration,
    ) -> Result<WorkTask, PlatformError> {
        if input.expected_revision < 1
            || input.state_mapping.is_empty()
            || input.state_mapping.len() > 200
        {
            return Err(PlatformError::Validation(
                "invalid workflow migration mapping",
            ));
        }
        DomainText::required(&input.reason, 4000)?;
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let scope = TaskScopeAuthority::task(&mut tx, ctx, id, Capability::TasksWrite)?;
        tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('technical_lead','pmo','manager','super_admin') FOR SHARE",&[&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Forbidden)?;
        let locked: bool = tx
            .query_one(
                "SELECT pg_try_advisory_xact_lock(hashtextextended($1,0))",
                &[&format!("task-transition:{id}")],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        if !locked {
            return Err(PlatformError::Conflict("a task transition is executing"));
        }
        crate::task_relation_lock::TaskRelationLock::acquire(&mut tx)?;
        let before = crate::task_policy_store::TaskPolicyStore::scoped_task(
            &mut tx,
            scope.project_id.as_deref(),
            id,
        )?;
        TaskScopeAuthority::modify(&mut tx, ctx, before.project_id.as_deref(), id, false)?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.workflow_migrate",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(v);
        }
        if before.revision != input.expected_revision
            || before.archived
            || before.lifecycle == TaskLifecycle::Completed
        {
            return Err(PlatformError::Conflict(
                "task revision changed or task archived",
            ));
        }
        let source = TaskPoolWorkflow::flow(
            &mut tx,
            before.project_id.as_deref(),
            before.execution_version()?,
            false,
        )?;
        let target = TaskPoolWorkflow::flow(
            &mut tx,
            before.project_id.as_deref(),
            &input.workflow_version_id,
            true,
        )?;
        if source.workflow_id != target.workflow_id || target.number <= source.number {
            return Err(PlatformError::Validation(
                "choose a newer version of the same workflow",
            ));
        }
        let state = input
            .state_mapping
            .get(before.execution_state()?)
            .ok_or(PlatformError::Validation("map the current task state"))?;
        let old = source
            .graph
            .states
            .iter()
            .find(|s| s.id == before.state_id.as_deref().unwrap_or(""))
            .ok_or(PlatformError::Storage)?;
        let new = target.graph.states.iter().find(|s| s.id == *state).ok_or(
            PlatformError::Validation("destination state does not exist"),
        )?;
        if old.terminal || old.terminal != new.terminal {
            return Err(PlatformError::Conflict(
                "migration cannot complete or reopen a workflow stage",
            ));
        }
        let busy:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM task_transition_runs WHERE task_id=$1 AND record->>'status' IN ('running','waiting_approval','waiting_external') OR task_id=$1 AND record->>'status'='failed' AND (COALESCE(record->'provider_intents','{}'::jsonb)<>'{}'::jsonb OR COALESCE(record->'outcomes','[]'::jsonb)<>'[]'::jsonb))",&[&id]).map_err(PlatformStore::database_error)?.get(0);
        if busy {
            return Err(PlatformError::Conflict(
                "resolve pending transition runs before migrating",
            ));
        }
        let related:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM task_subtasks WHERE removed_at IS NULL AND (parent_task_id=$1 OR child_task_id=$1))",&[&id]).map_err(PlatformStore::database_error)?.get(0);
        if related {
            return Err(PlatformError::Conflict(
                "related task stages must be migrated together; migrate an independent task",
            ));
        }
        if let Some(base) = &new.required_base_branch {
            let incompatible:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM task_repository_branches WHERE task_id=$1 AND base_branch<>$2)",&[&id,&base]).map_err(PlatformStore::database_error)?.get(0);
            if incompatible {
                return Err(PlatformError::Conflict(
                    "destination base branch differs from prepared task branches",
                ));
            }
        }
        let value:Value=tx.query_one("UPDATE tasks t SET workflow_version_id=$2,state_id=$3,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(t)",&[&id,&target.id,&state]).map_err(PlatformStore::database_error)?.get(0);
        let result: WorkTask = PlatformStore::decode(value)?;
        TaskHistoryWriter::append(
            &mut tx,
            ctx,
            &result,
            "task.workflow_migrated",
            Some(&before),
            Some(&input.reason),
        )?;
        ReceiptWriter::save(
            &mut tx,
            request.as_ref(),
            &serde_json::to_value(&result).map_err(|_| PlatformError::Storage)?,
        )?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
