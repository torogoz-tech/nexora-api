use nexora_graph::NodeKind;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyTarget {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<NodeKind>,
    #[serde(rename = "where")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<crate::PolicyFilter>,
}
