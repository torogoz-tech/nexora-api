use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpressionDefinition {
    pub ast_kind: String,
    pub identifiers: Vec<String>,
    pub literals: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub literal_selectors: Vec<crate::TokenSelector>,
    #[serde(default)]
    pub grouping: Vec<crate::DelimiterPair>,
    #[serde(default)]
    pub prefix: Vec<crate::UnaryOperator>,
    #[serde(default)]
    pub infix: Vec<crate::InfixOperator>,
    #[serde(default)]
    pub postfix: Vec<crate::UnaryOperator>,
    #[serde(default)]
    pub member: Vec<crate::UnaryOperator>,
    #[serde(default)]
    pub application: Vec<crate::ApplicationOperator>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditional: Vec<crate::ConditionalOperator>,
    #[serde(default)]
    pub assignment_targets: Vec<nexora_core::ExpressionKind>,
}

impl ExpressionDefinition {
    pub fn node_operator_texts(&self) -> impl Iterator<Item = &str> {
        self.prefix
            .iter()
            .chain(&self.postfix)
            .chain(&self.member)
            .map(|o| o.token.text.as_str())
            .chain(self.infix.iter().map(|o| o.token.text.as_str()))
            .chain(self.conditional.iter().map(|o| o.token.text.as_str()))
            .chain(self.grouping.iter().map(|g| g.open.as_str()))
            .chain(self.application.iter().map(|a| a.open.as_str()))
    }
}
