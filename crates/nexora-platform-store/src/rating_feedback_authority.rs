use crate::PlatformStore;
use nexora_platform::{AuthContext, PlatformError};
impl PlatformStore {
    pub(crate) fn rating_feedback_authority(
        tx: &mut postgres::Transaction<'_>,
        ctx: &AuthContext,
        target: &str,
    ) -> Result<(), PlatformError> {
        Self::rating_target(tx, target)?;
        // Feedback belongs exclusively to its recipient, including for administrators.
        if ctx.actor_id() == target {
            return Ok(());
        }
        Err(PlatformError::Forbidden)
    }
}
