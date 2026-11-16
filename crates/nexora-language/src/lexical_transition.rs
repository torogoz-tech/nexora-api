use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalTransition {
    pub pattern: String,
    pub region: String,
    #[serde(default)]
    pub reject: bool,
}
