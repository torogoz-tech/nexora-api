use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureAssessment {
    pub pack: String,
    pub feature: String,
    pub cases: usize,
    pub passed: usize,
    pub supported: usize,
    pub partial: usize,
    pub rejected: usize,
    pub status: String,
}
