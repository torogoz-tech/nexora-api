use crate::{EdgeKind, Evidence, ResolutionState};
use nexora_core::{EdgeId, NodeId};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEdge {
    pub id: EdgeId,
    pub source: NodeId,
    pub target: NodeId,
    pub kind: EdgeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_binding: Option<crate::ScopeBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_binding: Option<crate::CallBinding>,
    pub evidence: Evidence,
    pub resolution: ResolutionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}
