use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolutionState {
    Resolved,
    Ambiguous,
    Unresolved,
}
