use nexora_core::{NodeId, SourceSpan, SymbolId};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub scope_id: NodeId,
    pub ast_id: NodeId,
    pub span: SourceSpan,
}
