use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionCandidate {
    pub template: String,
    pub condition: crate::CandidateCondition,
    pub priority: u16,
}
