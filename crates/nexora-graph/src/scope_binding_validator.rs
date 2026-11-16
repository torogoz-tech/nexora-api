use crate::{CodeGraph, ComposedScopeIndex, EdgeKind, NodeKind, ResolutionState};
use nexora_core::NexoraError;
use std::collections::BTreeMap;
pub(crate) struct ScopeBindingValidator;
impl ScopeBindingValidator {
    pub fn validate(graph: &CodeGraph) -> Result<(), NexoraError> {
        if !graph.edges.iter().any(|e| e.scope_binding.is_some()) {
            return Ok(());
        }
        let fail = || NexoraError::Configuration("invalid composed lexical binding proof".into());
        let index = ComposedScopeIndex::new(graph)?;
        let lookup = index.lookup();
        let nodes: BTreeMap<_, _> = graph.nodes.iter().map(|n| (&n.id, n)).collect();
        let scopes: BTreeMap<_, _> = index.table.scopes.iter().map(|s| (&s.id, s)).collect();
        let mut budgets = BTreeMap::new();
        for edge in &graph.edges {
            let Some(binding) = &edge.scope_binding else {
                continue;
            };
            let site = nodes.get(&binding.site).ok_or_else(fail)?;
            let scope = scopes.get(&binding.scope_id).ok_or_else(fail)?;
            let source = nodes.get(&edge.source).ok_or_else(fail)?;
            let expected_scope = source.owned_scope_id.as_ref().or(source.scope_id.as_ref());
            let owner = scope
                .owner
                .as_ref()
                .and_then(|id| nodes.get(id))
                .ok_or_else(fail)?;
            if !matches!(edge.kind, EdgeKind::References | EdgeKind::Exports)
                || (edge.kind == EdgeKind::Exports && !binding.filter_before_ambiguity)
                || (binding.filter_before_ambiguity && site.kind != NodeKind::Symbol)
                || (!binding.filter_before_ambiguity
                    && site
                        .expression
                        .as_ref()
                        .is_none_or(|s| s.kind != nexora_core::ExpressionKind::Identifier))
                || site.name != binding.name
                || site.syntax_unit != edge.evidence.syntax_unit
                || site.evidence.as_ref().is_none_or(|e| {
                    e.ast_id != edge.evidence.ast_id
                        || e.span != edge.evidence.span
                        || e.path != edge.evidence.path
                })
                || (!binding.filter_before_ambiguity && binding.site != edge.source)
                || binding.name.is_empty()
                || binding.name.len() > 4096
                || binding.target_kinds.is_empty()
                || binding.target_kinds.len() > 128
                || expected_scope != Some(&binding.scope_id)
                || owner.syntax_unit != edge.evidence.syntax_unit
            {
                return Err(fail());
            }
            let (state, target) = index.resolve(
                &lookup,
                binding,
                edge.evidence.span,
                budgets
                    .entry(edge.evidence.path.as_str())
                    .or_insert(100_000),
            );
            if state != edge.resolution {
                return Err(fail());
            }
            let reason = match state {
                ResolutionState::Resolved => "composed_lexical_binding/v1",
                ResolutionState::Ambiguous => "composed_lexical_ambiguous/v1",
                ResolutionState::Unresolved => "composed_lexical_unresolved/v1",
            };
            if edge.resolution_reason.as_deref() != Some(reason) {
                return Err(fail());
            }
            if let Some(target) = target {
                if edge.target != target {
                    return Err(fail());
                }
            } else {
                let target = nodes.get(&edge.target).ok_or_else(fail)?;
                if target.kind != NodeKind::Symbol
                    || target.name != binding.name
                    || target.syntax_unit != edge.evidence.syntax_unit
                    || target.evidence.as_ref().is_none_or(|e| {
                        e.ast_id != edge.evidence.ast_id
                            || e.span != edge.evidence.span
                            || e.path != edge.evidence.path
                    })
                {
                    return Err(fail());
                }
            }
        }
        Ok(())
    }
}
