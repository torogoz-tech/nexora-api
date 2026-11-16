use nexora_governance::EvaluationStatus;
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyTestCase {
    pub name: String,
    pub policy: String,
    pub source: String,
    pub expected: EvaluationStatus,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub organization: Option<String>,
}
