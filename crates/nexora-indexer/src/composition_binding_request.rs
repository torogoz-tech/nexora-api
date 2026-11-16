use nexora_graph::{CodeNode, ScopeBinding};
pub(crate) struct CompositionBindingRequest {
    pub edge_index: usize,
    pub unit_index: usize,
    pub binding: ScopeBinding,
    pub placeholder: CodeNode,
}
