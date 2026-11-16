use nexora_core::ExpressionKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionFormFilter {
    pub kinds: Vec<ExpressionKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<crate::TokenSelector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descendant_kinds: Option<Vec<ExpressionKind>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descendant_operators: Option<Vec<String>>,
}
