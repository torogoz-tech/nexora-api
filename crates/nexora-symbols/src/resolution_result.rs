use nexora_core::SymbolId;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResolutionResult {
    Resolved(SymbolId),
    Ambiguous(Vec<SymbolId>),
    Unresolved,
}
