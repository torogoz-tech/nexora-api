use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernanceSnapshot {
    pub schema_version: u32,
    pub snapshot: nexora_storage::Snapshot,
    pub language: String,
    pub coverage: crate::AnalysisCoverage,
}
