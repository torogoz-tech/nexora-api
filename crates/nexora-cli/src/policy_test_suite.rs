use crate::policy_test_case::PolicyTestCase;
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PolicyTestSuite {
    pub api_version: String,
    pub kind: String,
    pub cases: Vec<PolicyTestCase>,
}
