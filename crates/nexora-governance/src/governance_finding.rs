use nexora_core::{AnalysisDiagnostic, NodeId};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernanceFinding {
    pub id: String,
    pub policy_id: String,
    pub policy_version: crate::PolicyVersion,
    pub severity: crate::PolicySeverity,
    pub node_id: Option<NodeId>,
    pub status: crate::EvaluationStatus,
    pub on_unknown: crate::UnknownBehavior,
    pub evidence: Vec<crate::GovernanceEvidence>,
    pub diagnostics: Vec<AnalysisDiagnostic>,
}
