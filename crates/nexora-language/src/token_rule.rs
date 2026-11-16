use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenRule {
    pub kind: String,
    #[serde(default = "default_enabled", skip_serializing_if = "is_enabled")]
    pub root: bool,
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_exclude: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<crate::TokenSelector>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after_kinds: Vec<String>,
    #[serde(default = "default_enabled")]
    pub at_start: bool,
    #[serde(default)]
    pub reject_on_context_mismatch: bool,
    #[serde(default)]
    pub trivia: bool,
    #[serde(default)]
    pub reject: bool,
}

fn default_enabled() -> bool {
    true
}

fn is_enabled(value: &bool) -> bool {
    *value
}
