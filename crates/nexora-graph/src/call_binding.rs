use crate::NodeKind;
use nexora_core::EdgeId;
use serde::{Deserialize, Serialize};
/// Model policy and actual callee reference used to prove a direct call.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallBinding {
    pub reference: Option<EdgeId>,
    pub callable_kinds: Vec<NodeKind>,
    pub unwrap_groups: bool,
}
