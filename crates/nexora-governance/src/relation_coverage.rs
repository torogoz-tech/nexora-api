use nexora_core::NodeId;
use nexora_graph::EdgeKind;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationCoverage {
    pub node_id: NodeId,
    pub relation: EdgeKind,
    pub exhaustive: bool,
    pub domain: String,
}
