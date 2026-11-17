use crate::{
    PlatformStore, platform_authority::PlatformAuthority, workflow_read_scope::WorkflowReadScope,
};
use nexora_platform::{AuthContext, AuthMethod, Capability, KeyScope, PlatformError};
use postgres::Transaction;
use serde_json::Value;
pub(crate) struct WorkflowReadAuthority;
impl WorkflowReadAuthority {
    pub fn scope(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
    ) -> Result<WorkflowReadScope, PlatformError> {
        // This validates the live identity and credential before considering membership.
        match PlatformAuthority::require(tx, ctx, Capability::WorkflowsRead, None) {
            Ok(()) => return Ok(WorkflowReadScope::All),
            Err(PlatformError::Forbidden) => {}
            Err(error) => return Err(error),
        }
        match PlatformAuthority::require(tx, ctx, Capability::WorkflowAssignmentsManage, None) {
            Ok(()) => return Ok(WorkflowReadScope::All),
            Err(PlatformError::Forbidden) => {}
            Err(error) => return Err(error),
        }
        if ctx.method() == AuthMethod::ApiKey {
            let row=tx.query_opt("SELECT scope FROM api_keys WHERE id=$1 AND actor_id=$2 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>clock_timestamp()) FOR SHARE",&[&ctx.credential_id(),&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
            let scope: KeyScope = PlatformStore::decode(row.get::<_, Value>(0))?;
            if !scope.capabilities.contains(&Capability::WorkflowsRead) {
                return Err(PlatformError::Forbidden);
            }
        }
        let rows=tx.query("SELECT workflow_id FROM developer_workflows WHERE actor_id=$1 ORDER BY workflow_id FOR SHARE",&[&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        Ok(if rows.is_empty() {
            WorkflowReadScope::Unassigned
        } else {
            WorkflowReadScope::Assigned(rows.iter().map(|row| row.get(0)).collect())
        })
    }
}
