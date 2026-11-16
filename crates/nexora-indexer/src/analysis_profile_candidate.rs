use nexora_language::AnalysisProfilePackage;
use serde::{Deserialize, Serialize};

/// Immutable imported root package, pinned by the job that supplies it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisProfileCandidate {
    pub id: String,
    pub checksum: String,
    /// Canonical UTC import time, used only after numeric version comparison.
    pub created_at: String,
    pub package: AnalysisProfilePackage,
}
