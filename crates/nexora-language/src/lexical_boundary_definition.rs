use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalBoundaryDefinition {
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after_kinds: Vec<String>,
}
