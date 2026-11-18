use crate::{
    SyntaxUnit, compiled_composition_profile::CompiledCompositionProfile,
    composition_budget::CompositionBudget, composition_call_request::CompositionCallRequest,
};
use nexora_core::{AnalysisDiagnostic, NexoraError, NodeId};
use nexora_graph::{
    CallBinding, CodeGraph, CodeNode, ComposedCallIndex, EdgeKind, NodeKind, ResolutionState,
};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) struct CompositionCallResolver;
impl CompositionCallResolver {
    pub fn apply(
        graph: &mut CodeGraph,
        units: &mut [SyntaxUnit],
        profiles: &BTreeMap<String, CompiledCompositionProfile>,
        budget: &mut CompositionBudget,
    ) -> Result<(), NexoraError> {
        if !profiles.values().any(|p| {
            p.language
                .manifest
                .composition
                .as_ref()
                .is_some_and(|c| c.direct_calls)
        }) {
            return Ok(());
        }
        let enabled: BTreeMap<_, _> = units
            .iter()
            .map(|u| (u.id.as_str(), u.direct_calls))
            .collect();
        for origin in graph.syntax_units.values_mut() {
            origin.direct_calls = enabled
                .get(origin.unit_id.as_str())
                .copied()
                .unwrap_or(false);
        }
        let requests = {
            let index = ComposedCallIndex::new(graph)?;
            budget.account(0, graph.nodes.len())?;
            let unit_indexes: BTreeMap<_, _> = units
                .iter()
                .enumerate()
                .map(|(i, u)| (u.id.as_str(), i))
                .collect();
            let ast: BTreeMap<_, _> = units
                .iter()
                .flat_map(|u| u.ast.nodes.iter().map(move |n| ((u.id.as_str(), &n.id), n)))
                .collect();
            let mut remaining = 100_000;
            let mut requests = Vec::new();
            for (edge_index, edge) in graph
                .edges
                .iter()
                .enumerate()
                .filter(|(_, e)| e.kind == EdgeKind::Calls)
            {
                let Some(origin) = edge
                    .evidence
                    .syntax_unit
                    .as_ref()
                    .and_then(|id| graph.syntax_units.get(id))
                else {
                    continue;
                };
                let Some(&unit_index) = unit_indexes.get(origin.unit_id.as_str()) else {
                    continue;
                };
                let unit = &units[unit_index];
                let profile = &profiles[&unit.language.0].language;
                if !profile
                    .manifest
                    .composition
                    .as_ref()
                    .is_some_and(|c| c.direct_calls)
                {
                    continue;
                }
                let node = ast
                    .get(&(unit.id.as_str(), &edge.evidence.ast_id))
                    .ok_or_else(|| {
                        NexoraError::Configuration("call has no original AST site".into())
                    })?;
                let rule = profile
                    .graph
                    .expression_bindings
                    .get(&node.kind)
                    .ok_or_else(|| {
                        NexoraError::Configuration("call has no declared binding policy".into())
                    })?;
                let callable_kinds: Vec<_> = rule
                    .callable_kinds
                    .iter()
                    .filter_map(|k| NodeKind::parse(k))
                    .collect();
                let decision = index.decide(
                    &edge.source,
                    &callable_kinds,
                    rule.unwrap_groups,
                    &mut remaining,
                )?;
                let binding = CallBinding {
                    reference: decision.reference.clone(),
                    callable_kinds,
                    unwrap_groups: rule.unwrap_groups,
                };
                requests.push(CompositionCallRequest {
                    edge_index,
                    unit_index,
                    binding,
                    decision,
                });
            }
            requests
        };
        budget.account(0, requests.len())?;
        let symbols: BTreeSet<_> = graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Symbol)
            .map(|n| n.id.clone())
            .collect();
        let mut touched = BTreeSet::new();
        for request in requests {
            let edge = &mut graph.edges[request.edge_index];
            if symbols.contains(&edge.target) {
                touched.insert(edge.target.clone());
            }
            edge.target = if let Some(target) = request.decision.target {
                target
            } else {
                budget.account(0, 1)?;
                let id = NodeId(format!("composed-direct-call:{}", edge.id.0));
                graph.nodes.push(CodeNode {
                    expression_policy: None,
                    id: id.clone(),
                    kind: NodeKind::Symbol,
                    syntax_unit: edge.evidence.syntax_unit.clone(),
                    name: String::new(),
                    expression: None,
                    mutation_operand: None,
                    lexical_region: None,
                    evidence: Some(edge.evidence.clone()),
                    scope_id: None,
                    owned_scope_id: None,
                    scope_policy: None,
                    regions: Vec::new(),
                    captures: Default::default(),
                });
                id
            };
            edge.resolution = request.decision.state;
            edge.resolution_reason = Some(request.decision.reason.into());
            edge.call_binding = Some(request.binding);
            let diagnostics = &mut units[request.unit_index].diagnostics;
            diagnostics.retain(|d| {
                !(matches!(
                    d.stage.as_str(),
                    "expression-resolution" | "composed-symbol-resolution"
                ) && d.span == Some(edge.evidence.span))
            });
            if edge.resolution != ResolutionState::Resolved && diagnostics.len() < 1024 {
                diagnostics.push(AnalysisDiagnostic {
                    stage: "composed-call-resolution".into(),
                    path: Some(edge.evidence.path.clone()),
                    span: Some(edge.evidence.span),
                    message: request.decision.reason.into(),
                });
            }
        }
        let used: BTreeSet<_> = graph
            .edges
            .iter()
            .flat_map(|e| [&e.source, &e.target])
            .chain(
                graph
                    .edges
                    .iter()
                    .filter_map(|e| e.scope_binding.as_ref().map(|b| &b.site)),
            )
            .cloned()
            .collect();
        graph
            .nodes
            .retain(|n| !touched.contains(&n.id) || used.contains(&n.id));
        nexora_graph::GraphIntegrityValidator::validate(graph)?;
        Ok(())
    }
}
