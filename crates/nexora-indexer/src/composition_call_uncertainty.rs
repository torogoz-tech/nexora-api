use crate::{SyntaxUnit, composition_budget::CompositionBudget};
use nexora_core::{AnalysisDiagnostic, ExpressionKind, NexoraError, NodeId};
use nexora_graph::{CodeGraph, CodeNode, EdgeKind, NodeKind, ResolutionState};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) struct CompositionCallUncertainty;
impl CompositionCallUncertainty {
    pub fn apply(
        graph: &mut CodeGraph,
        units: &mut [SyntaxUnit],
        budget: &mut CompositionBudget,
    ) -> Result<(), NexoraError> {
        let nodes: BTreeMap<_, _> = graph
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n.clone()))
            .collect();
        let mut writes = BTreeSet::new();
        let mut foreign_identifiers = BTreeSet::new();
        let mut callee_states = BTreeMap::new();
        let mut uncertain_write = false;
        for edge in graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::References && e.scope_binding.is_some())
        {
            callee_states.insert(edge.source.clone(), edge.resolution.clone());
            let foreign = nodes
                .get(&edge.target)
                .is_some_and(|n| n.syntax_unit != edge.evidence.syntax_unit);
            if foreign && edge.resolution == ResolutionState::Resolved {
                foreign_identifiers.insert(edge.source.clone());
                if edge.label.as_deref() == Some("write") {
                    writes.insert(edge.target.clone());
                }
            } else if edge.label.as_deref() == Some("write")
                && edge.resolution != ResolutionState::Resolved
                && edge
                    .evidence
                    .syntax_unit
                    .as_ref()
                    .and_then(|id| nodes.get(id))
                    .and_then(|n| n.scope_policy.as_ref())
                    .is_some_and(|p| p.parent_lookup)
            {
                uncertain_write = true;
            }
        }
        let mut remaining = 100_000usize;
        for edge in graph.edges.iter_mut().filter(|e| e.kind == EdgeKind::Calls) {
            if remaining == 0 {
                return Err(NexoraError::Limit("composed call inspection budget".into()));
            }
            remaining -= 1;
            let mut callee = nodes
                .get(&edge.source)
                .and_then(|n| n.expression.as_ref())
                .and_then(|s| s.operands.first())
                .and_then(|id| nodes.get(id));
            for _ in 0..256 {
                if remaining == 0 {
                    return Err(NexoraError::Limit("composed call inspection budget".into()));
                }
                remaining -= 1;
                if callee
                    .and_then(|n| n.expression.as_ref())
                    .is_none_or(|s| s.kind != ExpressionKind::Group)
                {
                    break;
                }
                callee = callee
                    .and_then(|n| n.expression.as_ref())
                    .and_then(|s| s.operands.first())
                    .and_then(|id| nodes.get(id));
            }
            let callee_state = callee.and_then(|n| callee_states.get(&n.id));
            let lost_binding = callee_state.is_some_and(|s| *s != ResolutionState::Resolved);
            let pending = callee.is_some_and(|n| foreign_identifiers.contains(&n.id));
            let invalidate = edge.resolution == ResolutionState::Resolved
                && (uncertain_write || writes.contains(&edge.target));
            if !pending && !invalidate && !lost_binding {
                continue;
            }
            let reason = if lost_binding {
                "composed_callee_binding_not_unique"
            } else if invalidate {
                "composed_mutation_prevents_call_certainty"
            } else {
                "composed_call_resolution_pending"
            };
            if edge.resolution == ResolutionState::Resolved && (invalidate || lost_binding) {
                budget.account(0, 1)?;
                let id = NodeId(format!("composed-call:{}", edge.id.0));
                graph.nodes.push(CodeNode {
                    expression_policy: None,
                    mutation_operand: None,
                    id: id.clone(),
                    kind: NodeKind::Symbol,
                    name: nodes
                        .get(&edge.source)
                        .map_or_else(String::new, |n| n.name.clone()),
                    syntax_unit: edge.evidence.syntax_unit.clone(),
                    expression: None,
                    lexical_region: None,
                    evidence: Some(edge.evidence.clone()),
                    scope_id: None,
                    owned_scope_id: None,
                    scope_policy: None,
                    regions: Vec::new(),
                    captures: Default::default(),
                });
                edge.target = id;
            }
            edge.resolution = if lost_binding {
                callee_state.cloned().unwrap_or(ResolutionState::Unresolved)
            } else {
                ResolutionState::Unresolved
            };
            edge.resolution_reason = Some(reason.into());
            if let Some(unit) = edge
                .evidence
                .syntax_unit
                .as_ref()
                .and_then(|id| graph.syntax_units.get(id))
                .and_then(|origin| units.iter_mut().find(|u| u.id == origin.unit_id))
            {
                unit.diagnostics.retain(|d| {
                    !(d.stage == "expression-resolution" && d.span == Some(edge.evidence.span))
                });
                if unit.diagnostics.len() < 1024 {
                    unit.diagnostics.push(AnalysisDiagnostic {
                        stage: "composed-symbol-resolution".into(),
                        path: Some(edge.evidence.path.clone()),
                        span: Some(edge.evidence.span),
                        message: reason.into(),
                    });
                }
            }
        }
        Ok(())
    }
}
