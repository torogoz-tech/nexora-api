use crate::PlatformStore;
use nexora_platform::PlatformError;
impl PlatformStore {
    pub(crate) fn rating_general_authority(
        tx: &mut postgres::Transaction<'_>,
        actor: &str,
    ) -> Result<(), PlatformError> {
        let allowed = tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('developer','technical_lead','pmo','manager','super_admin') FOR SHARE", &[&actor]).map_err(Self::database_error)?.is_some();
        if allowed {
            Ok(())
        } else {
            Err(PlatformError::Forbidden)
        }
    }
}
