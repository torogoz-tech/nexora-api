use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalRegionDefinition {
    pub close: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub close_before: Vec<crate::LexicalBoundaryDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub token_rules: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_kind: Option<String>,
    #[serde(default)]
    pub trivia: bool,
    #[serde(default)]
    pub opening_follow: Option<String>,
    #[serde(default)]
    pub close_capture: Option<String>,
    #[serde(default)]
    pub close_tail: Option<String>,
    #[serde(default)]
    pub close_ascii_case_insensitive: bool,
    #[serde(default)]
    pub header: Option<crate::LexicalRegionHeader>,
    #[serde(default)]
    pub recovery_prefixes: Vec<String>,
    #[serde(default = "default_recovery_barrier")]
    pub recovery_barrier: bool,
    #[serde(default)]
    pub alternate_closes: Vec<String>,
    #[serde(default)]
    pub escape: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub literal_escapes: Vec<String>,
    #[serde(default)]
    pub children: Vec<crate::LexicalTransition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counted_children: Vec<crate::LexicalCountedTransition>,
    #[serde(default)]
    pub eof_closes: bool,
    #[serde(default = "default_ast_kind")]
    pub ast_kind: String,
}
fn default_ast_kind() -> String {
    "lexical_region".into()
}

fn default_recovery_barrier() -> bool {
    true
}
