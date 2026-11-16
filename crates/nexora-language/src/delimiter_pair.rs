use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelimiterPair {
    pub open: String,
    pub close: String,
    #[serde(default)]
    pub kind: Option<String>,
}
