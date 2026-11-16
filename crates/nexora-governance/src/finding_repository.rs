use crate::{EvaluationResult, GovernanceError};
use nexora_core::{RepositoryId, TenantId};
/// Result storage is separate from source snapshots and policy source-of-truth.
pub trait FindingRepository {
    fn save(&mut self, result: &EvaluationResult) -> Result<(), GovernanceError>;
    fn get(
        &self,
        tenant: &TenantId,
        repository: &RepositoryId,
        evaluation: &str,
    ) -> Result<Option<EvaluationResult>, GovernanceError>;
}
