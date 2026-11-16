use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributePredicate {
    pub attribute: String,
    pub operator: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}
