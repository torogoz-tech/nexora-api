use crate::platform_authority::PlatformAuthority;
use nexora_platform::{AuthContext, Capability, PlatformError};
use postgres::Transaction;
pub(crate) struct DeveloperWorkflowAuthority;
impl DeveloperWorkflowAuthority {
    pub fn require(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        write: bool,
    ) -> Result<(), PlatformError> {
        match PlatformAuthority::require(tx, ctx, Capability::WorkflowAssignmentsManage, None) {
            Ok(()) => Ok(()),
            Err(PlatformError::Forbidden) => {
                PlatformAuthority::require(tx, ctx, Capability::ActorsManage, None)?;
                if write {
                    PlatformAuthority::require(tx, ctx, Capability::WorkflowsWrite, None)?;
                }
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
}
