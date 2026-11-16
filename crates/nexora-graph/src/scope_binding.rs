use crate::NodeKind;
use nexora_core::NodeId;
use serde::{Deserialize, Serialize};
/// Declared lexical lookup attached to its original reference evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeBinding {
    pub name: String,
    pub site: NodeId,
    pub scope_id: NodeId,
    pub target_kinds: Vec<NodeKind>,
    pub filter_before_ambiguity: bool,
}
