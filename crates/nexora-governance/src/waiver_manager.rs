use crate::{GovernanceError, WaiverRequest};
/// Contract only. Implementations must enforce authorization externally.
/// Evaluator does not apply waivers or provide an approval flow in this release.
pub trait WaiverManager {
    fn authorized_exception(&self, request: &WaiverRequest) -> Result<bool, GovernanceError>;
}
