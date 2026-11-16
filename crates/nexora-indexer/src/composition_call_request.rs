use nexora_graph::{CallBinding, ComposedCallDecision};
pub(crate) struct CompositionCallRequest {
    pub edge_index: usize,
    pub unit_index: usize,
    pub binding: CallBinding,
    pub decision: ComposedCallDecision,
}
