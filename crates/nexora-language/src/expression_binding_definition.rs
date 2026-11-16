use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionBindingDefinition {
    pub target_kinds: Vec<String>,
    pub callable_kinds: Vec<String>,
    #[serde(default)]
    pub unwrap_groups: bool,
    #[serde(default)]
    pub mutating_unary_operators: Vec<String>,
}
