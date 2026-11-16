use nexora_core::{NodeId, SourceSpan};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcreteSyntaxNode {
    pub id: NodeId,
    pub production: String,
    pub ast_kind: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lexical_region: Option<nexora_core::LexicalRegion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<nexora_core::ExpressionShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<NodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<nexora_core::ScopePolicy>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub captures: std::collections::BTreeMap<String, nexora_core::SyntaxCapture>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<nexora_core::SyntaxRegion>,
    #[serde(default)]
    pub role: nexora_core::SyntaxRole,
    pub span: SourceSpan,
    pub token_start: usize,
    pub token_end: usize,
}
