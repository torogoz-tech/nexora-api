use crate::{
    PlatformStore, receipt_writer::ReceiptWriter, task_history_writer::TaskHistoryWriter,
    task_policy_store::TaskPolicyStore, task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::*;
impl PlatformStore {
    pub fn set_task_paused(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        revision: i64,
        paused: bool,
    ) -> Result<WorkTask, PlatformError> {
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        crate::task_relation_lock::TaskRelationLock::acquire(&mut tx)?;
        let task = TaskScopeAuthority::task(&mut tx, ctx, id, Capability::TasksTransition)?;
        let before = TaskPolicyStore::scoped_task(&mut tx, task.project_id.as_deref(), id)?;
        let manager = tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('manager','super_admin') FOR SHARE", &[&ctx.actor_id()]).map_err(Self::database_error)?.is_some();
        if before.assignee_id.as_deref() != Some(ctx.actor_id()) && !(paused && manager) {
            return Err(PlatformError::Forbidden);
        }
        let operation = if paused { "task.pause" } else { "task.resume" };
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            operation,
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Self::decode(value);
        }
        if revision != before.revision {
            return Err(PlatformError::Conflict("task revision changed"));
        }
        let expected = if paused {
            TaskLifecycle::Active
        } else {
            TaskLifecycle::Paused
        };
        if before.archived || before.lifecycle != expected {
            return Err(PlatformError::Conflict(
                "task is not in the required execution state",
            ));
        }
        if !paused {
            crate::task_relation_policy::TaskRelationPolicy::take(&mut tx, &before)?;
            if before.workflow_id.is_some() {
                tx.query_opt("SELECT actor_id FROM developer_workflows WHERE actor_id=$1 AND workflow_id=$2 FOR SHARE", &[&ctx.actor_id(), &before.workflow_id])
                .map_err(Self::database_error)?.ok_or(PlatformError::Conflict("workflow membership is no longer available"))?;
            }
        }
        let lifecycle = if paused { "paused" } else { "active" };
        let value: serde_json::Value = tx.query_one("UPDATE tasks AS t SET lifecycle=$2,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(t)", &[&id,&lifecycle]).map_err(Self::database_error)?.get(0);
        let task: WorkTask = Self::decode(value)?;
        TaskHistoryWriter::append(&mut tx, ctx, &task, operation, Some(&before), None)?;
        ReceiptWriter::save(
            &mut tx,
            request.as_ref(),
            &serde_json::to_value(&task).map_err(|_| PlatformError::Storage)?,
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(task)
    }
}
