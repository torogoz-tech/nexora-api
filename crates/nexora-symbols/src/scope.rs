use nexora_core::NodeId;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scope {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<NodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<nexora_core::ScopePolicy>,
}
