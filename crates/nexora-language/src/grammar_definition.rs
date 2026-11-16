use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrammarDefinition {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_scope: Option<nexora_core::ScopePolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub root_productions: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub contexts: BTreeMap<String, crate::GrammarContext>,
    pub max_nesting: usize,
    pub max_steps: usize,
    pub productions: Vec<crate::ProductionRule>,
    #[serde(default)]
    pub rules: BTreeMap<String, crate::GrammarPattern>,
    #[serde(default)]
    pub groups: Vec<crate::DelimiterPair>,
    #[serde(default)]
    pub ignored: Vec<crate::GrammarPattern>,
    #[serde(default)]
    pub opaque_references: Vec<crate::TokenSelector>,
    #[serde(default)]
    pub recovery: Vec<crate::TokenSelector>,
    #[serde(default)]
    pub recover_line_break: bool,
    #[serde(default)]
    pub recovery_groups: Vec<String>,
}
