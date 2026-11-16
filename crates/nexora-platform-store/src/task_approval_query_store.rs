use crate::PlatformStore;
use nexora_platform::{AuthContext, Capability, PlatformError, TaskApproval};
use serde_json::Value;

pub(crate) struct TaskApprovalQueryStore;
impl TaskApprovalQueryStore {
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        task: &str,
        id: &str,
    ) -> Result<TaskApproval, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        crate::task_scope_authority::TaskScopeAuthority::require(
            &mut tx,
            ctx,
            Capability::TasksRead,
            project,
        )
        .map_err(|error| {
            if matches!(error, PlatformError::Forbidden) {
                PlatformError::NotFound
            } else {
                error
            }
        })?;
        crate::task_scope_authority::TaskScopeAuthority::relation(&mut tx, ctx, project, task)?;
        let value:Value=tx.query_opt("SELECT to_jsonb(a) FROM task_approvals a WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2 AND task_id=$3",&[&id,&project,&task]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
