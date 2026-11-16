use crate::{
    PlatformStore, receipt_writer::ReceiptWriter, task_history_writer::TaskHistoryWriter,
    task_policy_store::TaskPolicyStore,
};
use nexora_platform::*;
use serde_json::{Value, json};

pub(crate) struct TaskApprovalRevocationStore;
impl TaskApprovalRevocationStore {
    pub fn revoke(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        task: &str,
        id: &str,
        input: &TaskApprovalRevoke,
    ) -> Result<TaskApproval, PlatformError> {
        if ctx.method() != AuthMethod::Session {
            return Err(PlatformError::Forbidden);
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        crate::task_scope_authority::TaskScopeAuthority::require(
            &mut tx,
            ctx,
            Capability::ApprovalsGrant,
            project,
        )?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.approval_revoke",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        // Same task-before-approval lock order as transition/consumption.
        let resource = TaskPolicyStore::scoped_task(&mut tx, project, task)?;
        let value:Value=tx.query_opt("SELECT to_jsonb(a) FROM task_approvals a WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2 AND task_id=$3 FOR UPDATE",&[&id,&project,&task]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        let before: TaskApproval = PlatformStore::decode(value)?;
        if before.consumed_at.is_some() || before.revoked_at.is_some() {
            return Err(PlatformError::Conflict(
                "approval already consumed or revoked",
            ));
        }
        let value:Value=tx.query_one("UPDATE task_approvals AS a SET revoked_at=clock_timestamp(),revoked_by=$2,revocation_reason=$3 WHERE id=$1 RETURNING to_jsonb(a)",&[&id,&ctx.actor_id(),&reason]).map_err(PlatformStore::database_error)?.get(0);
        TaskHistoryWriter::event(
            &mut tx,
            ctx,
            &resource,
            "task.approval_revoked",
            json!({"approval_id":id,"reason":reason}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
