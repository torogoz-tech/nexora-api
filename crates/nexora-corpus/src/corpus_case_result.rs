use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusCaseResult {
    pub id: String,
    pub pack: String,
    pub feature: String,
    pub classification: crate::CorpusClassification,
    pub benchmark: bool,
    pub fingerprint: String,
    pub passed: bool,
    pub scores: crate::CorpusScores,
    pub expected_facts: usize,
    pub observed_facts: usize,
    pub source_bytes: usize,
    pub diagnostics: usize,
    pub analysis: crate::AnalysisExpectation,
    pub policy: Option<nexora_governance::EvaluationStatus>,
    pub policy_expected: Option<nexora_governance::EvaluationStatus>,
    pub invalid_evidence: usize,
    pub token_reconstruction: Option<bool>,
    pub cst_ast_traceability: Option<bool>,
    pub setup_ms: f64,
    pub analysis_elapsed_ms: f64,
    pub policy_ms: f64,
    pub assertions_ms: f64,
    pub total_ms: f64,
    pub pipeline: Option<nexora_indexer::StageTimings>,
    pub mismatches: Vec<crate::CorpusMismatch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<crate::CorpusFact>,
}
