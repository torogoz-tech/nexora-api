use crate::*;
use nexora_core::{RepositoryId, TenantId};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct MemoryFindingRepository {
    results: BTreeMap<(TenantId, RepositoryId, String), EvaluationResult>,
}
impl FindingRepository for MemoryFindingRepository {
    fn save(&mut self, result: &EvaluationResult) -> Result<(), GovernanceError> {
        GovernanceReporter::verify(result)?;
        self.results.insert(
            (
                result.tenant_id.clone(),
                result.repository_id.clone(),
                result.id.clone(),
            ),
            result.clone(),
        );
        Ok(())
    }
    fn get(
        &self,
        tenant: &TenantId,
        repository: &RepositoryId,
        evaluation: &str,
    ) -> Result<Option<EvaluationResult>, GovernanceError> {
        Ok(self
            .results
            .get(&(tenant.clone(), repository.clone(), evaluation.into()))
            .cloned())
    }
}
