use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrammarContext {
    #[serde(default, skip_serializing_if = "is_false")]
    pub stop_on_error: bool,
    #[serde(default)]
    pub productions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<crate::ExpressionDefinition>,
    #[serde(default)]
    pub ignored: Vec<crate::GrammarPattern>,
}

fn is_false(value: &bool) -> bool {
    !value
}
