use crate::{CodeGraph, EdgeKind, ResolutionState, ScopeBridgeOrigins};
use nexora_core::NexoraError;
use std::collections::BTreeSet;
pub(crate) struct ScopeBridgeValidator;
impl ScopeBridgeValidator {
    pub fn validate(graph: &CodeGraph) -> Result<(), NexoraError> {
        let fail = || NexoraError::Configuration("invalid declared lexical scope bridge".into());
        let nodes: std::collections::BTreeMap<_, _> =
            graph.nodes.iter().map(|n| (&n.id, n)).collect();
        let expected = ScopeBridgeOrigins::select(graph);
        let mut targets = BTreeSet::new();
        for edge in graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::ScopeBridge)
        {
            if nodes
                .get(&edge.target)
                .is_none_or(|n| n.owned_scope_id.is_none())
                || !targets.insert(&edge.target)
                || edge.resolution != ResolutionState::Resolved
                || edge.resolution_reason.as_deref() != Some("declared_scope_bridge/v1")
                || edge.scope_binding.is_some()
                || !expected.iter().any(|origin| {
                    origin.source == edge.source
                        && origin.target == edge.target
                        && origin.label == edge.label
                        && origin.evidence.ast_id == edge.evidence.ast_id
                        && origin.evidence.span == edge.evidence.span
                        && origin.evidence.syntax_unit == edge.evidence.syntax_unit
                })
            {
                return Err(fail());
            }
        }
        Ok(())
    }
}
