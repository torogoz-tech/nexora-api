use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusReport {
    pub schema_version: u32,
    pub corpus_id: String,
    pub corpus_version: String,
    pub corpus_fingerprint: String,
    pub analysis_engine: String,
    pub governance_engine: String,
    pub build: String,
    pub os: String,
    pub architecture: String,
    pub passed: bool,
    pub scores: crate::CorpusScores,
    pub conformance_scores: crate::CorpusScores,
    pub benchmark_scores: crate::CorpusScores,
    pub loading_ms: f64,
    pub cases: Vec<crate::CorpusCaseResult>,
    pub features: Vec<crate::FeatureAssessment>,
}
