use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CriterionResult {
    pub criterion_id: String,
    pub outcome: String,
    pub explanation: String,
    pub evidence_ids: Vec<String>,
    pub missing_evidence: Vec<String>,
}
