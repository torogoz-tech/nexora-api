use crate::ResolutionState;
use nexora_core::NodeId;
pub(crate) struct ExpressionBindingResult {
    pub target: Option<NodeId>,
    pub state: ResolutionState,
    pub reason: &'static str,
}
impl ExpressionBindingResult {
    pub fn unresolved(reason: &'static str) -> Self {
        Self {
            target: None,
            state: ResolutionState::Unresolved,
            reason,
        }
    }
}
