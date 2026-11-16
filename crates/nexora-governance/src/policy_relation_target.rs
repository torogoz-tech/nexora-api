use nexora_graph::NodeKind;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRelationTarget {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<NodeKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "fileMatches", skip_serializing_if = "Option::is_none")]
    pub file_matches: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherits: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implements: Option<String>,
}
