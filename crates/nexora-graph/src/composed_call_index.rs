use crate::{
    CodeEdge, CodeGraph, CodeNode, ComposedCallDecision as Decision, ComposedMutationIndex,
    EdgeKind, NodeKind, ResolutionState,
};
use nexora_core::{ExpressionKind as K, NexoraError, NodeId, RegionStatus};
use std::collections::BTreeMap;
pub struct ComposedCallIndex<'a> {
    graph: &'a CodeGraph,
    nodes: BTreeMap<&'a NodeId, &'a CodeNode>,
    references: BTreeMap<&'a NodeId, &'a CodeEdge>,
    mutations: ComposedMutationIndex,
}
impl<'a> ComposedCallIndex<'a> {
    pub fn new(graph: &'a CodeGraph) -> Result<Self, NexoraError> {
        let mut references = BTreeMap::new();
        for edge in graph.edges.iter().filter(|e| {
            e.kind == EdgeKind::References
                && e.scope_binding
                    .as_ref()
                    .is_some_and(|b| !b.filter_before_ambiguity)
        }) {
            if references.insert(&edge.source, edge).is_some() {
                return Err(NexoraError::Configuration(
                    "duplicate callee reference evidence".into(),
                ));
            }
        }
        Ok(Self {
            graph,
            nodes: graph.nodes.iter().map(|n| (&n.id, n)).collect(),
            references,
            mutations: ComposedMutationIndex::new(graph)?,
        })
    }
    pub fn decide(
        &self,
        source: &NodeId,
        callable_kinds: &[NodeKind],
        unwrap_groups: bool,
        remaining: &mut usize,
    ) -> Result<Decision, NexoraError> {
        let fail = || NexoraError::Configuration("invalid direct call application evidence".into());
        let application = self.nodes.get(source).ok_or_else(fail)?;
        let shape = application
            .expression
            .as_ref()
            .filter(|s| s.kind == K::Application)
            .ok_or_else(fail)?;
        let mut callee = shape
            .operands
            .first()
            .and_then(|id| self.nodes.get(id))
            .copied();
        let mut depth = 0;
        while unwrap_groups
            && callee
                .and_then(|n| n.expression.as_ref())
                .is_some_and(|s| s.kind == K::Group)
        {
            if *remaining == 0 || depth >= 256 {
                return Err(NexoraError::Limit(
                    "composed direct call inspection budget".into(),
                ));
            }
            *remaining -= 1;
            depth += 1;
            callee = callee
                .and_then(|n| n.expression.as_ref())
                .and_then(|s| s.operands.first())
                .and_then(|id| self.nodes.get(id))
                .copied();
        }
        if *remaining == 0 {
            return Err(NexoraError::Limit(
                "composed direct call inspection budget".into(),
            ));
        }
        *remaining -= 1;
        let source_complete = application
            .syntax_unit
            .as_ref()
            .and_then(|id| self.graph.syntax_units.get(id))
            .is_some_and(|u| {
                u.status == RegionStatus::Parsed && u.mutation_analysis && u.direct_calls
            });
        let Some(callee) = callee.filter(|n| {
            n.expression
                .as_ref()
                .is_some_and(|s| s.kind == K::Identifier)
        }) else {
            return Ok(Decision::unresolved(
                "composed_indirect_or_member_callee",
                None,
            ));
        };
        let Some(reference) = self.references.get(&callee.id) else {
            return Ok(Decision::unresolved(
                "composed_callee_reference_unavailable",
                None,
            ));
        };
        let evidence = Some(reference.id.clone());
        if !source_complete {
            return Ok(Decision::unresolved(
                "composed_incomplete_call_context",
                evidence,
            ));
        }
        if reference.resolution != ResolutionState::Resolved {
            return Ok(Decision {
                target: None,
                reference: evidence,
                state: reference.resolution.clone(),
                reason: "composed_callee_binding_not_unique",
            });
        }
        let target = self.nodes.get(&reference.target).ok_or_else(fail)?;
        if !callable_kinds.contains(&target.kind) {
            return Ok(Decision::unresolved(
                "composed_binding_not_directly_callable",
                evidence,
            ));
        }
        if target
            .syntax_unit
            .as_ref()
            .and_then(|id| self.graph.syntax_units.get(id))
            .is_none_or(|u| u.status != RegionStatus::Parsed || !u.mutation_analysis)
        {
            return Ok(Decision::unresolved(
                "composed_incomplete_callable_context",
                evidence,
            ));
        }
        if self.mutations.written.contains(&target.id) {
            return Ok(Decision::unresolved(
                "composed_binding_is_mutated",
                evidence,
            ));
        }
        if target
            .scope_id
            .as_ref()
            .is_none_or(|id| self.mutations.uncertain_scopes.contains(id))
        {
            return Ok(Decision::unresolved(
                "composed_uncertain_mutation",
                evidence,
            ));
        }
        Ok(Decision {
            target: Some(target.id.clone()),
            reference: evidence,
            state: ResolutionState::Resolved,
            reason: "unique_composed_direct_callable/v1",
        })
    }
}
