use crate::platform_authority::PlatformAuthority;
use nexora_platform::{ActorKind, AuthContext, AuthMethod, Capability, PlatformError};
use postgres::Transaction;

pub(crate) struct LocalSourceOperatorAuthority;
impl LocalSourceOperatorAuthority {
    pub fn lock(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
    ) -> Result<(), PlatformError> {
        if ctx.kind() != ActorKind::Human || ctx.method() != AuthMethod::Session {
            return Err(PlatformError::Forbidden);
        }
        PlatformAuthority::require(tx, ctx, Capability::InstallationManage, None)?;
        PlatformAuthority::require(tx, ctx, Capability::RepositoriesWrite, Some(project))?;
        PlatformAuthority::project(tx, project)
    }
}
