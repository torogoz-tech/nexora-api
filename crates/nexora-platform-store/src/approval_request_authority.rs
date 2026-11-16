use crate::platform_authority::PlatformAuthority;
use nexora_platform::*;
use postgres::Transaction;
pub(crate) struct ApprovalRequestAuthority;
impl ApprovalRequestAuthority {
    pub fn allows(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        cap: Capability,
    ) -> Result<bool, PlatformError> {
        match PlatformAuthority::require(tx, ctx, cap, Some(project)) {
            Ok(()) => Ok(true),
            Err(PlatformError::Forbidden) => Ok(false),
            Err(error) => Err(error),
        }
    }
    pub fn readable(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
    ) -> Result<(bool, bool), PlatformError> {
        let approve = Self::allows(tx, ctx, project, Capability::ApprovalsGrant)?;
        let tasks = Self::allows(tx, ctx, project, Capability::TasksRead)?;
        let documents = Self::allows(tx, ctx, project, Capability::DocumentsRead)?;
        if !approve && !tasks && !documents {
            return Err(PlatformError::NotFound);
        }
        PlatformAuthority::project(tx, project)?;
        Ok((approve || tasks, approve || documents))
    }
    pub fn human(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
    ) -> Result<(), PlatformError> {
        if ctx.method() != AuthMethod::Session {
            return Err(PlatformError::Forbidden);
        }
        PlatformAuthority::require(tx, ctx, Capability::ApprovalsGrant, Some(project))?;
        PlatformAuthority::project(tx, project)
    }
}
