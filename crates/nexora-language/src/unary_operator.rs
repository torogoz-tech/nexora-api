use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnaryOperator {
    pub token: crate::TokenSelector,
    pub precedence: u16,
}
