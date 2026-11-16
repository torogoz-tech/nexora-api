use nexora_governance::EvaluationStatus;
use serde::Serialize;
#[derive(Serialize)]
pub struct PolicyTestResult {
    pub name: String,
    pub policy: String,
    pub expected: EvaluationStatus,
    pub actual: EvaluationStatus,
    pub passed: bool,
    pub evaluation_id: String,
}
