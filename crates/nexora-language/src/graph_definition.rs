use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphDefinition {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub expression_bindings: BTreeMap<String, crate::ExpressionBindingDefinition>,
    pub mappings: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reference_relations: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub symbol_references: BTreeMap<String, crate::SymbolReferenceDefinition>,
}
