use crate::{Evidence, NodeKind};
use nexora_core::NodeId;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syntax_unit: Option<NodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lexical_region: Option<nexora_core::LexicalRegion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<nexora_core::ExpressionShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation_operand: Option<NodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression_policy: Option<String>,
    pub evidence: Option<Evidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<NodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_scope_id: Option<NodeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_policy: Option<nexora_core::ScopePolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<nexora_core::SyntaxRegion>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub captures: std::collections::BTreeMap<String, nexora_core::SyntaxCapture>,
}
