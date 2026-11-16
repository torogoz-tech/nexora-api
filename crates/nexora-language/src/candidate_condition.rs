use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateCondition {
    Always,
    WithoutExtension,
}
