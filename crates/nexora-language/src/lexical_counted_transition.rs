use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LexicalCountedTransition {
    pub capture: String,
    pub unit: String,
    pub open: String,
    pub region: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub open_at_end: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_run_factor: Option<u16>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reject_unmatched_close: bool,
}
