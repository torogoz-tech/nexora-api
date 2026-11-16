use crate::ResolutionState;
use nexora_core::{EdgeId, NodeId};
pub struct ComposedCallDecision {
    pub target: Option<NodeId>,
    pub reference: Option<EdgeId>,
    pub state: ResolutionState,
    pub reason: &'static str,
}
impl ComposedCallDecision {
    pub(crate) fn unresolved(reason: &'static str, reference: Option<EdgeId>) -> Self {
        Self {
            target: None,
            reference,
            state: ResolutionState::Unresolved,
            reason,
        }
    }
}
