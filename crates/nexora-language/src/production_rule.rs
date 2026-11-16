use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionRule {
    pub name: String,
    pub pattern: crate::GrammarPattern,
    pub projection: crate::SyntaxProjection,
}
