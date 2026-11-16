use crate::{CodeGraph, EdgeKind, NodeKind, ResolutionState};
use nexora_core::{AnalysisDiagnostic, NodeId};
use nexora_language::SymbolReferenceDefinition;
use std::collections::{BTreeMap, BTreeSet};
pub(crate) struct LocalSymbolResolver;
impl LocalSymbolResolver {
    pub fn resolve(
        graph: &mut CodeGraph,
        strategies: &BTreeMap<NodeId, SymbolReferenceDefinition>,
        complete: bool,
        symbols: &nexora_symbols::SymbolTable,
    ) -> Vec<AnalysisDiagnostic> {
        if strategies.is_empty() {
            return Vec::new();
        }
        let lookup = nexora_symbols::SymbolLookup::new(symbols);
        let nodes: BTreeMap<_, _> = graph.nodes.iter().map(|n| (n.id.clone(), n)).collect();
        let mut declarations: BTreeMap<(NodeId, String), Vec<NodeId>> = BTreeMap::new();
        for edge in graph.edges.iter().filter(|e| e.kind == EdgeKind::Declares) {
            if let Some(target) = nodes.get(&edge.target) {
                declarations
                    .entry((edge.source.clone(), target.name.clone()))
                    .or_default()
                    .push(target.id.clone());
            }
        }
        let mut references: BTreeMap<NodeId, Vec<usize>> = BTreeMap::new();
        for (i, edge) in graph.edges.iter().enumerate() {
            if strategies.contains_key(&edge.target) {
                references.entry(edge.target.clone()).or_default().push(i);
            }
        }
        let mut remaining = 100_000usize;
        let mut diagnostics = Vec::new();
        let mut removed = BTreeSet::new();
        for (id, edges) in references {
            if diagnostics.len() >= 1024 {
                break;
            }
            let Some(node) = nodes.get(&id) else {
                continue;
            };
            let Some(strategy) = strategies.get(&id) else {
                continue;
            };
            let Some(first) = edges.first().and_then(|i| graph.edges.get(*i)) else {
                continue;
            };
            let mut candidates = Vec::new();
            let mut exhausted = remaining == 0;
            remaining = remaining.saturating_sub(1);
            if complete && !exhausted {
                let lexical;
                let available: &[NodeId] = match strategy.scope {
                    nexora_language::SymbolScope::File => declarations
                        .get(&(
                            NodeId(format!("file:{}", first.evidence.path)),
                            node.name.clone(),
                        ))
                        .map_or(&[], Vec::as_slice),
                    nexora_language::SymbolScope::Lexical => {
                        let scope = node
                            .evidence
                            .as_ref()
                            .and_then(|e| symbols.node_scopes.get(&e.ast_id))
                            .cloned()
                            .unwrap_or_else(|| NodeId("scope:file".into()));
                        match lookup.candidates(
                            &nexora_symbols::SymbolReference {
                                name: node.name.clone(),
                                scope_id: scope,
                                span: first.evidence.span,
                            },
                            &mut remaining,
                        ) {
                            Some(found) => {
                                lexical = found
                                    .iter()
                                    .map(|s| {
                                        NodeId(format!("{}:{}", first.evidence.path, s.ast_id.0))
                                    })
                                    .collect::<Vec<_>>()
                            }
                            None => {
                                exhausted = true;
                                lexical = Vec::new();
                            }
                        }
                        &lexical
                    }
                };
                for candidate in available {
                    if remaining == 0 {
                        exhausted = true;
                        break;
                    }
                    remaining -= 1;
                    if nodes.get(candidate).is_some_and(|n| {
                        strategy
                            .target_kinds
                            .iter()
                            .any(|k| NodeKind::parse(k) == Some(n.kind.clone()))
                    }) {
                        candidates.push(candidate.clone());
                        if candidates.len() == 2 {
                            break;
                        }
                    }
                }
            }
            let state = if !complete || exhausted {
                ResolutionState::Unresolved
            } else {
                match candidates.len() {
                    1 => ResolutionState::Resolved,
                    0 => ResolutionState::Unresolved,
                    _ => ResolutionState::Ambiguous,
                }
            };
            for i in &edges {
                graph.edges[*i].resolution = state.clone();
                if state == ResolutionState::Resolved {
                    graph.edges[*i].target = candidates[0].clone();
                }
            }
            if state == ResolutionState::Resolved {
                removed.insert(id);
            } else {
                diagnostics.push(AnalysisDiagnostic {
                    stage: "symbol-resolution".into(),
                    path: node.evidence.as_ref().map(|e| e.path.clone()),
                    span: node.evidence.as_ref().map(|e| e.span),
                    message: if !complete {
                        "symbol lookup has incomplete syntax regions or parsing diagnostics"
                    } else if exhausted {
                        "local symbol lookup budget exhausted"
                    } else if state == ResolutionState::Ambiguous {
                        "ambiguous declared symbol in configured scope"
                    } else {
                        "no declared symbol matches the configured name and target kinds"
                    }
                    .into(),
                });
            }
        }
        graph.nodes.retain(|n| !removed.contains(&n.id));
        diagnostics
    }
}
