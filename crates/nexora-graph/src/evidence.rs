use nexora_core::{NodeId, SourceSpan};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub path: String,
    pub span: SourceSpan,
    pub ast_id: NodeId,
    /// ast_id belongs to this unit when present; otherwise it belongs to the file AST.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syntax_unit: Option<NodeId>,
    pub rule_version: String,
}
