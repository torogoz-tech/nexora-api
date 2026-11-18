use crate::{
    SyntaxUnit, compiled_composition_profile::CompiledCompositionProfile,
    composition_binding_request::CompositionBindingRequest, composition_budget::CompositionBudget,
    unit_graph_namespace::UnitGraphNamespace,
};
use nexora_core::{AnalysisDiagnostic, ExpressionKind, NexoraError, NodeId};
use nexora_graph::{
    CodeGraph, CodeNode, ComposedScopeIndex, EdgeKind, NodeKind, ResolutionState, ScopeBinding,
};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) struct CompositionBindingResolver;
impl CompositionBindingResolver {
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
                .is_some_and(|c| c.schema_version >= 4)
        }) {
            return Ok(());
        }
        let index = ComposedScopeIndex::new(graph)?;
        budget.account(0, index.table.scopes.len() + index.table.symbols.len())?;
        let lookup = index.lookup();
        let mut requests = Vec::new();
        let unit_indexes: BTreeMap<_, _> = units
            .iter()
            .enumerate()
            .map(|(i, u)| (u.id.as_str(), i))
            .collect();
        let ast_nodes: BTreeMap<_, _> = units
            .iter()
            .flat_map(|u| u.ast.nodes.iter().map(move |n| ((u.id.as_str(), &n.id), n)))
            .collect();
        for (edge_index, edge) in graph
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e.kind, EdgeKind::References | EdgeKind::Exports))
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
            let Some(node) = ast_nodes.get(&(unit.id.as_str(), &edge.evidence.ast_id)) else {
                continue;
            };
            let profile = &profiles[&unit.language.0].language;
            let (kinds, filter_before_ambiguity) = if node
                .expression
                .as_ref()
                .is_some_and(|s| s.kind == ExpressionKind::Identifier)
            {
                let Some(rule) = profile.graph.expression_bindings.get(&node.kind) else {
                    continue;
                };
                (&rule.target_kinds, false)
            } else {
                let Some(rule) = profile.graph.symbol_references.get(&node.kind) else {
                    continue;
                };
                if !matches!(rule.scope, nexora_language::SymbolScope::Lexical) {
                    continue;
                }
                (&rule.target_kinds, true)
            };
            let Some(scope) = unit.symbols.node_scopes.get(&node.id) else {
                continue;
            };
            let namespace = UnitGraphNamespace::new(&origin.path, &unit.id, unit_index == 0);
            let binding = ScopeBinding {
                name: node.name.clone(),
                site: if filter_before_ambiguity {
                    NodeId(format!("composed-binding:{}", edge.id.0))
                } else {
                    edge.source.clone()
                },
                scope_id: namespace.scope(&NodeId(format!("{}:{}", origin.path, scope.0))),
                target_kinds: kinds
                    .iter()
                    .filter_map(|kind| NodeKind::parse(kind))
                    .collect(),
                filter_before_ambiguity,
            };
            let placeholder = CodeNode {
                expression_policy: None,
                mutation_operand: None,
                id: NodeId(format!("composed-binding:{}", edge.id.0)),
                kind: NodeKind::Symbol,
                name: node.name.clone(),
                syntax_unit: edge.evidence.syntax_unit.clone(),
                expression: None,
                lexical_region: None,
                evidence: Some(edge.evidence.clone()),
                scope_id: Some(binding.scope_id.clone()),
                owned_scope_id: None,
                scope_policy: None,
                regions: Vec::new(),
                captures: Default::default(),
            };
            requests.push(CompositionBindingRequest {
                edge_index,
                unit_index,
                binding,
                placeholder,
            });
        }
        budget.account(0, requests.len())?;
        let old_symbols: BTreeSet<_> = graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Symbol)
            .map(|n| n.id.clone())
            .collect();
        let mut touched = BTreeSet::new();
        let mut remaining = 100_000;
        for request in requests {
            let edge = &mut graph.edges[request.edge_index];
            if old_symbols.contains(&edge.target) {
                touched.insert(edge.target.clone());
            }
            let (state, target) = index.resolve(
                &lookup,
                &request.binding,
                edge.evidence.span,
                &mut remaining,
            );
            if request.binding.filter_before_ambiguity {
                budget.account(0, 1)?;
                graph.nodes.push(request.placeholder.clone());
            }
            edge.target = if let Some(id) = target {
                id
            } else {
                if !request.binding.filter_before_ambiguity {
                    budget.account(0, 1)?;
                    graph.nodes.push(request.placeholder.clone());
                }
                request.placeholder.id
            };
            edge.resolution_reason = Some(
                match state {
                    ResolutionState::Resolved => "composed_lexical_binding/v1",
                    ResolutionState::Ambiguous => "composed_lexical_ambiguous/v1",
                    ResolutionState::Unresolved => "composed_lexical_unresolved/v1",
                }
                .into(),
            );
            edge.resolution = state;
            edge.scope_binding = Some(request.binding);
            let diagnostics = &mut units[request.unit_index].diagnostics;
            diagnostics.retain(|d| {
                !(matches!(
                    d.stage.as_str(),
                    "expression-resolution" | "symbol-resolution"
                ) && d.span == Some(edge.evidence.span))
            });
            if edge.resolution != ResolutionState::Resolved && diagnostics.len() < 1024 {
                diagnostics.push(AnalysisDiagnostic {
                    stage: "composed-symbol-resolution".into(),
                    path: Some(edge.evidence.path.clone()),
                    span: Some(edge.evidence.span),
                    message: edge.resolution_reason.clone().unwrap_or_default(),
                });
            }
        }
        let used: BTreeSet<_> = graph
            .edges
            .iter()
            .flat_map(|e| [&e.source, &e.target])
            .cloned()
            .collect();
        graph
            .nodes
            .retain(|n| !touched.contains(&n.id) || used.contains(&n.id));
        crate::composition_call_uncertainty::CompositionCallUncertainty::apply(
            graph, units, budget,
        )?;
        nexora_graph::GraphIntegrityValidator::validate(graph)?;
        Ok(())
    }
}
