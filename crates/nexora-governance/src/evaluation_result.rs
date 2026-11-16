use nexora_core::{RepositoryId, SnapshotId, TenantId};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationResult {
    pub id: String,
    pub engine_version: String,
    pub policy_set_hash: String,
    pub tenant_id: TenantId,
    pub repository_id: RepositoryId,
    pub snapshot_id: SnapshotId,
    pub language: String,
    pub coverage: crate::AnalysisCoverage,
    pub analysis_diagnostics: Vec<nexora_core::AnalysisDiagnostic>,
    pub status: crate::EvaluationStatus,
    pub findings: Vec<crate::GovernanceFinding>,
}
