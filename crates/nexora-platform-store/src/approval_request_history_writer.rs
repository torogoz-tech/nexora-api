use crate::{
    document_history_writer::DocumentHistoryWriter, task_history_writer::TaskHistoryWriter,
    task_policy_store::TaskPolicyStore,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::json;
pub(crate) struct ApprovalRequestHistoryWriter;
impl ApprovalRequestHistoryWriter {
    pub fn append(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        request: &ApprovalRequest,
        event: &str,
        reason: &str,
    ) -> Result<(), PlatformError> {
        let details = json!({"request_id":request.id,"approval_id":request.approval_id,"operation_checksum":request.operation_checksum,"status":request.status,"reason":reason});
        match &request.target {
            ApprovalRequestTarget::Task { task_id, .. } => {
                let task = TaskPolicyStore::task(tx, &request.project_id, task_id)?;
                TaskHistoryWriter::event(tx, ctx, &task, event, details)
            }
            ApprovalRequestTarget::DocumentVersion {
                document_id,
                version_id,
                ..
            } => DocumentHistoryWriter::append(
                tx,
                ctx,
                &request.project_id,
                document_id,
                version_id,
                event,
                reason,
                details,
            ),
        }
    }
}
