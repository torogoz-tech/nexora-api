use crate::PlatformStore;
use nexora_platform::{ActorKind, AuthContext, AuthMethod, PlatformError};
use postgres::Transaction;
pub(crate) struct MemberAvatarAuthority;
impl MemberAvatarAuthority {
    pub fn read(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        member: &str,
    ) -> Result<(), PlatformError> {
        crate::labor_calendar_policy::LaborCalendarPolicy::session(tx, ctx)?;
        tx.query_opt("SELECT a.id FROM actors a JOIN users u ON u.actor_id=a.id WHERE a.id=$1 AND a.kind='human' FOR SHARE OF a,u", &[&member]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        Ok(())
    }
    pub fn change(tx: &mut Transaction<'_>, ctx: &AuthContext) -> Result<(), PlatformError> {
        if ctx.kind() != ActorKind::Human || ctx.method() != AuthMethod::Session {
            return Err(PlatformError::Forbidden);
        }
        Self::read(tx, ctx, ctx.actor_id())
    }
}
