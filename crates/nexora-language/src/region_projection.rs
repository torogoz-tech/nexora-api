use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionProjection {
    pub capture: String,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub trim_start: usize,
    #[serde(default)]
    pub trim_end: usize,
}
