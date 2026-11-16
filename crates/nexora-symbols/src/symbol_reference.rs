use nexora_core::{NodeId, SourceSpan};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolReference {
    pub name: String,
    pub scope_id: NodeId,
    pub span: SourceSpan,
}
