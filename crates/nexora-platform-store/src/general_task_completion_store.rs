use crate::{
    PlatformStore, receipt_writer::ReceiptWriter, task_history_writer::TaskHistoryWriter,
    task_policy_store::TaskPolicyStore, task_relation_policy::TaskRelationPolicy,
    task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::*;
impl PlatformStore {
    pub fn complete_general_task(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        revision: i64,
    ) -> Result<WorkTask, PlatformError> {
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        crate::task_relation_lock::TaskRelationLock::acquire(&mut tx)?;
        let scope = TaskScopeAuthority::task(&mut tx, ctx, id, Capability::TasksTransition)?;
        let before = TaskPolicyStore::scoped_task(&mut tx, scope.project_id.as_deref(), id)?;
        if before.assignee_id.as_deref() != Some(ctx.actor_id()) {
            return Err(PlatformError::Forbidden);
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.general_complete",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Self::decode(value);
        }
        if before.revision != revision {
            return Err(PlatformError::Conflict("task revision changed"));
        }
        if before.purpose != TaskPurpose::General
            || before.entry_workflow_id.is_some()
            || before.workflow_id.is_some()
            || before.workflow_version_id.is_some()
            || before.state_id.is_some()
        {
            return Err(PlatformError::Conflict(
                "only general tasks without a workflow can complete directly",
            ));
        }
        if before.archived || before.lifecycle != TaskLifecycle::Active {
            return Err(PlatformError::Conflict(
                "only active general tasks can complete",
            ));
        }
        TaskRelationPolicy::take(&mut tx, &before)?;
        TaskRelationPolicy::state(&mut tx, &before, true)?;
        let value:serde_json::Value=tx.query_one("UPDATE tasks AS t SET lifecycle='completed',revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(t)",&[&id]).map_err(Self::database_error)?.get(0);
        let task: WorkTask = Self::decode(value.clone())?;
        TaskHistoryWriter::append(
            &mut tx,
            ctx,
            &task,
            "task.general_complete",
            Some(&before),
            None,
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(task)
    }
}
