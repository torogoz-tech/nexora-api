use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusExpectation {
    pub analysis: crate::AnalysisExpectation,
    #[serde(default)]
    pub facts: Vec<crate::CorpusFact>,
    #[serde(default)]
    pub policy: Option<nexora_governance::EvaluationStatus>,
    #[serde(default)]
    pub coverage: Option<Vec<nexora_core::SourceCoverage>>,
}
