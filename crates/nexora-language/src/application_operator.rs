use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationOperator {
    pub open: String,
    pub close: String,
    pub kind: Option<String>,
    pub separator: crate::TokenSelector,
    pub precedence: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression_kind: Option<nexora_core::ExpressionKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_arguments: Option<u16>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_receivers: Vec<crate::ExpressionFormFilter>,
}
