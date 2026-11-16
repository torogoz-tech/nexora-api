use nexora_core::{EdgeId, NodeId, SourceSpan};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernanceEvidence {
    pub predicate: String,
    pub result: crate::EvaluationStatus,
    pub polarity: String,
    pub node_id: Option<NodeId>,
    pub edge_ids: Vec<EdgeId>,
    pub path: Option<String>,
    pub span: Option<SourceSpan>,
    pub inspected_scope: String,
    pub exhaustive: bool,
    pub message: String,
}
