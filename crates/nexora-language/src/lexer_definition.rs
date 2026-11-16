use crate::TokenRule;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexerDefinition {
    pub schema_version: u32,
    pub identifier_kind: String,
    pub keywords: Vec<String>,
    pub tokens: Vec<TokenRule>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub entry_contexts: std::collections::BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyword_after_exclusions: Vec<crate::TokenSelector>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub regions: std::collections::BTreeMap<String, crate::LexicalRegionDefinition>,
    #[serde(default = "default_region_steps")]
    pub max_region_steps: usize,
    #[serde(default = "default_region_depth")]
    pub max_region_depth: usize,
    #[serde(default = "default_region_nodes")]
    pub max_region_nodes: usize,
}

fn default_region_steps() -> usize {
    2_000_000
}
fn default_region_depth() -> usize {
    128
}

fn default_region_nodes() -> usize {
    100_000
}
