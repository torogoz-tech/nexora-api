use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionalOperator {
    pub token: crate::TokenSelector,
    pub separator: crate::TokenSelector,
    pub precedence: u16,
    pub consequent_precedence: u16,
    pub alternative_precedence: u16,
}
