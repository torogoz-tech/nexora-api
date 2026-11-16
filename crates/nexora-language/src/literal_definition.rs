use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiteralDefinition {
    pub mode: crate::LiteralMode,
    #[serde(default)]
    pub quotes: Vec<String>,
    #[serde(default)]
    pub escape: Option<String>,
    #[serde(default)]
    pub escapes: BTreeMap<String, String>,
    #[serde(default)]
    pub escaped_quote: bool,
    #[serde(default)]
    pub escaped_escape: bool,
}
