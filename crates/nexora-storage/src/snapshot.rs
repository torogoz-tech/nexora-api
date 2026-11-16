use nexora_core::{AnalysisDiagnostic, RepositoryId, SnapshotId, TenantId};
use nexora_graph::CodeGraph;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub tenant_id: TenantId,
    pub repository_id: RepositoryId,
    pub id: SnapshotId,
    pub graph: CodeGraph,
    pub sources: BTreeMap<String, String>,
    pub diagnostics: Vec<AnalysisDiagnostic>,
    pub rule_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis_engine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis_configuration: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_coverage: Vec<nexora_core::SourceCoverage>,
}
