use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReasoningRequest {
    pub requirement: Option<crate::reasoning_requirement::ReasoningRequirement>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    pub snapshot: Option<String>,
    pub baseline_snapshot: Option<String>,
    pub analysis: nexora_reasoning::AnalysisRequest,
}
