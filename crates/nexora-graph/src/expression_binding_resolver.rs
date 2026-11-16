use crate::{
    CodeGraph, EdgeKind, ResolutionState,
    expression_binding_emitter::ExpressionBindingEmitter as Emit,
    expression_binding_result::ExpressionBindingResult as Binding,
};
use nexora_ast::{AbstractSyntaxNode, AbstractSyntaxTree};
use nexora_core::{AnalysisDiagnostic, ExpressionKind as K, NodeId};
use nexora_language::LanguageDefinition;
use nexora_symbols::{SymbolLookup, SymbolReference, SymbolTable};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) struct ExpressionBindingResolver;
impl ExpressionBindingResolver {
    pub fn resolve(
        graph: &mut CodeGraph,
        ast: &AbstractSyntaxTree,
        symbols: &SymbolTable,
        path: &str,
        language: &LanguageDefinition,
        complete: bool,
    ) -> Vec<AnalysisDiagnostic> {
        if language.graph.expression_bindings.is_empty() {
            return Vec::new();
        }
        let nodes: BTreeMap<_, _> = ast.nodes.iter().map(|n| (&n.id, n)).collect();
        let lookup = SymbolLookup::new(symbols);
        let mut remaining = 100_000usize;
        let mut emitter = Emit {
            graph,
            path,
            version: &language.manifest.version,
            diagnostics: Vec::new(),
        };
        let declaration_kinds: BTreeMap<_, _> = ast
            .nodes
            .iter()
            .filter(|n| n.role == nexora_core::SyntaxRole::Declaration)
            .filter_map(|n| {
                language
                    .graph
                    .mappings
                    .get(&n.kind)
                    .map(|kind| (NodeId(format!("{path}:{}", n.id.0)), kind.as_str()))
            })
            .collect();
        let mut results = BTreeMap::new();
        let mut mutated = BTreeSet::new();
        let writes = crate::expression_write_summary::ExpressionWriteSummary::collect(
            &nodes,
            &language.graph,
            &mut remaining,
        );
        let mut uncertain_writes = writes.uncertain;
        for node in &ast.nodes {
            let Some(rule) = language.graph.expression_bindings.get(&node.kind) else {
                continue;
            };
            let Some(shape) = &node.expression else {
                continue;
            };
            if shape.kind != K::Identifier {
                continue;
            }
            let parent = node
                .parent_id
                .as_ref()
                .and_then(|id| nodes.get(id))
                .copied();
            if parent
                .and_then(|p| p.expression.as_ref())
                .is_some_and(|p| p.kind == K::Member && p.operands.get(1) == Some(&node.id))
            {
                continue;
            }
            let write = writes.identifiers.contains(&node.id);
            let result = if !complete {
                Binding::unresolved("incomplete_syntax")
            } else if remaining == 0 {
                Binding::unresolved("binding_budget_exhausted")
            } else {
                remaining -= 1;
                let reference = SymbolReference {
                    name: node.name.clone(),
                    scope_id: symbols
                        .node_scopes
                        .get(&node.id)
                        .cloned()
                        .unwrap_or_else(|| NodeId("scope:file".into())),
                    span: node.span,
                };
                match lookup.candidates(&reference, &mut remaining) {
                    None => Binding::unresolved("binding_budget_exhausted"),
                    Some(candidates) => match candidates.as_slice() {
                        [] => Binding::unresolved("missing_declaration"),
                        [symbol] => {
                            let allowed = nodes
                                .get(&symbol.ast_id)
                                .and_then(|n| language.graph.mappings.get(&n.kind))
                                .is_some_and(|k| rule.target_kinds.contains(k));
                            if allowed {
                                Binding {
                                    target: Some(NodeId(format!("{path}:{}", symbol.ast_id.0))),
                                    state: ResolutionState::Resolved,
                                    reason: "unique_lexical_binding",
                                }
                            } else {
                                Binding::unresolved("binding_kind_not_enabled")
                            }
                        }
                        _ => Binding {
                            target: None,
                            state: ResolutionState::Ambiguous,
                            reason: "ambiguous_lexical_binding",
                        },
                    },
                }
            };
            if write {
                if let Some(id) = &result.target {
                    mutated.insert(id.clone());
                } else {
                    uncertain_writes = true;
                }
            }
            emitter.emit(
                node,
                EdgeKind::References,
                if write { "write" } else { "read" },
                &result,
            );
            results.insert(node.id.clone(), result);
        }
        // Calls use the completed write summary, so source order cannot create false certainty.
        for node in &ast.nodes {
            let Some(rule) = language.graph.expression_bindings.get(&node.kind) else {
                continue;
            };
            let Some(shape) = &node.expression else {
                continue;
            };
            if shape.kind != K::Application {
                continue;
            }
            let mut callee = shape.operands.first().and_then(|id| nodes.get(id)).copied();
            let mut depth = 0;
            while rule.unwrap_groups
                && callee
                    .and_then(|n| n.expression.as_ref())
                    .is_some_and(|e| e.kind == K::Group)
            {
                if remaining == 0 || depth >= 256 {
                    break;
                }
                remaining -= 1;
                depth += 1;
                callee = callee
                    .and_then(|n| n.expression.as_ref())
                    .and_then(|e| e.operands.first())
                    .and_then(|id| nodes.get(id))
                    .copied();
            }
            let result = if !complete {
                Binding::unresolved("incomplete_syntax")
            } else if remaining == 0 {
                Binding::unresolved("binding_budget_exhausted")
            } else {
                remaining -= 1;
                Self::call(
                    callee,
                    &results,
                    &declaration_kinds,
                    rule,
                    &mutated,
                    uncertain_writes,
                )
            };
            emitter.emit(node, EdgeKind::Calls, "direct-call", &result);
        }
        emitter.diagnostics
    }
    fn call(
        callee: Option<&AbstractSyntaxNode>,
        results: &BTreeMap<NodeId, Binding>,
        declaration_kinds: &BTreeMap<NodeId, &str>,
        rule: &nexora_language::ExpressionBindingDefinition,
        mutated: &BTreeSet<NodeId>,
        uncertain_writes: bool,
    ) -> Binding {
        let Some(callee) = callee else {
            return Binding::unresolved("missing_callee");
        };
        if callee
            .expression
            .as_ref()
            .is_none_or(|e| e.kind != K::Identifier)
        {
            return Binding::unresolved("indirect_or_member_callee");
        }
        let Some(binding) = results.get(&callee.id) else {
            return Binding::unresolved("callee_binding_unavailable");
        };
        let Some(target) = &binding.target else {
            return Binding {
                target: None,
                state: binding.state.clone(),
                reason: binding.reason,
            };
        };
        let allowed = declaration_kinds
            .get(target)
            .is_some_and(|kind| rule.callable_kinds.iter().any(|k| k == kind));
        if !allowed {
            Binding::unresolved("binding_not_directly_callable")
        } else if uncertain_writes {
            Binding::unresolved("uncertain_mutation")
        } else if mutated.contains(target) {
            Binding::unresolved("binding_is_mutated")
        } else {
            Binding {
                target: Some(target.clone()),
                state: ResolutionState::Resolved,
                reason: "unique_direct_callable",
            }
        }
    }
}
