use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InfixOperator {
    pub token: crate::TokenSelector,
    pub precedence: u16,
    pub associativity: crate::OperatorAssociativity,
    pub kind: nexora_core::ExpressionKind,
}
