use crate::{CodeGraph, ComposedCallIndex, EdgeKind, NodeKind};
use nexora_core::NexoraError;
use std::collections::BTreeMap;
pub(crate) struct CallBindingValidator;
impl CallBindingValidator {
    pub fn validate(graph: &CodeGraph) -> Result<(), NexoraError> {
        if graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .any(|e| {
                e.evidence
                    .syntax_unit
                    .as_ref()
                    .and_then(|id| graph.syntax_units.get(id))
                    .is_some_and(|u| u.direct_calls)
                    && e.call_binding.is_none()
            })
        {
            return Err(NexoraError::Configuration(
                "missing composed direct call proof".into(),
            ));
        }
        if !graph.edges.iter().any(|e| e.call_binding.is_some()) {
            return Ok(());
        }
        let fail = || NexoraError::Configuration("invalid composed direct call proof".into());
        let index = ComposedCallIndex::new(graph)?;
        let nodes: BTreeMap<_, _> = graph.nodes.iter().map(|n| (&n.id, n)).collect();
        let mut budgets = BTreeMap::new();
        for edge in &graph.edges {
            let Some(binding) = &edge.call_binding else {
                continue;
            };
            let source = nodes.get(&edge.source).ok_or_else(fail)?;
            if source
                .syntax_unit
                .as_ref()
                .and_then(|id| graph.syntax_units.get(id))
                .is_none_or(|u| !u.direct_calls)
                || edge.kind != EdgeKind::Calls
                || edge.scope_binding.is_some()
                || binding.callable_kinds.len() > 16
                || binding
                    .callable_kinds
                    .iter()
                    .enumerate()
                    .any(|(i, kind)| binding.callable_kinds[..i].contains(kind))
                || source.evidence.as_ref().is_none_or(|e| {
                    e.ast_id != edge.evidence.ast_id
                        || e.span != edge.evidence.span
                        || e.path != edge.evidence.path
                })
            {
                return Err(fail());
            }
            let policy = source
                .expression_policy
                .as_ref()
                .and_then(|id| graph.expression_policies.get(id))
                .ok_or_else(fail)?;
            let kinds: Vec<_> = policy
                .callable_kinds
                .iter()
                .filter_map(|k| NodeKind::parse(k))
                .collect();
            if kinds != binding.callable_kinds || policy.unwrap_groups != binding.unwrap_groups {
                return Err(fail());
            }
            let decision = index.decide(
                &edge.source,
                &binding.callable_kinds,
                binding.unwrap_groups,
                budgets
                    .entry(edge.evidence.path.as_str())
                    .or_insert(100_000),
            )?;
            if decision.reference != binding.reference
                || decision.state != edge.resolution
                || edge.resolution_reason.as_deref() != Some(decision.reason)
            {
                return Err(fail());
            }
            if let Some(target) = decision.target {
                if target != edge.target {
                    return Err(fail());
                }
            } else {
                let target = nodes.get(&edge.target).ok_or_else(fail)?;
                if target.kind != NodeKind::Symbol
                    || target.syntax_unit != edge.evidence.syntax_unit
                    || target.evidence.as_ref().is_none_or(|e| {
                        e.ast_id != edge.evidence.ast_id || e.span != edge.evidence.span
                    })
                {
                    return Err(fail());
                }
            }
        }
        Ok(())
    }
}
