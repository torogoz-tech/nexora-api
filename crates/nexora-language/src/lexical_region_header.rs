use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalRegionHeader {
    pub end: String,
    #[serde(default)]
    pub terminal: Option<String>,
    #[serde(default)]
    pub escape: Option<String>,
    #[serde(default)]
    pub children: Vec<crate::LexicalTransition>,
}
