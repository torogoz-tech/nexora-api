use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssertionGroup {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all: Option<Vec<crate::PolicyAssertion>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub any: Option<Vec<crate::PolicyAssertion>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub none: Option<Vec<crate::PolicyAssertion>>,
}
