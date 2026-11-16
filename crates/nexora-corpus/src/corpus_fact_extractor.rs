use crate::CorpusFact;
use nexora_core::SourcePosition;
use nexora_graph::{CodeGraph, EdgeKind, Evidence, NodeKind};
use std::collections::BTreeMap;
pub struct CorpusFactExtractor;
impl CorpusFactExtractor {
    pub fn snippet<'a>(e: &Evidence, sources: &'a BTreeMap<String, String>) -> Option<&'a str> {
        Self::span_text(&e.path, e.span, sources).filter(|t| !t.is_empty())
    }
    fn span_text<'a>(
        path: &str,
        span: nexora_core::SourceSpan,
        sources: &'a BTreeMap<String, String>,
    ) -> Option<&'a str> {
        let source = sources.get(path)?;
        let before = source.get(..span.start.offset)?;
        let text = source.get(span.start.offset..span.end.offset)?;
        let mut start = SourcePosition::default();
        start.advance(before);
        let mut end = start;
        end.advance(text);
        (start == span.start && end == span.end).then_some(text)
    }
    pub fn extract(
        graph: &CodeGraph,
        sources: &BTreeMap<String, String>,
    ) -> (Vec<CorpusFact>, usize) {
        let nodes: BTreeMap<_, _> = graph.nodes.iter().map(|n| (&n.id, n)).collect();
        let parents: BTreeMap<_, _> = graph
            .edges
            .iter()
            .filter(|e| matches!(e.kind, EdgeKind::Declares | EdgeKind::Contains))
            .map(|e| (&e.target, &e.source))
            .collect();
        let ancestry = |id: &nexora_core::NodeId| {
            let mut id = id;
            let mut path = Vec::new();
            let mut visited = std::collections::BTreeSet::new();
            while let Some(node) = nodes.get(id) {
                if node.kind == NodeKind::File {
                    break;
                }
                if !visited.insert(id) || visited.len() > 256 {
                    break;
                }
                if node.expression.is_none() {
                    path.push(node.name.clone());
                }
                let Some(parent) = parents.get(id) else {
                    break;
                };
                id = parent;
            }
            path.reverse();
            path
        };
        let valid_units: std::collections::BTreeSet<_> = graph
            .syntax_units
            .iter()
            .filter(|(_, unit)| Self::span_text(&unit.path, unit.span, sources).is_some())
            .map(|(id, _)| id)
            .collect();
        let mut facts = Vec::new();
        let mut invalid = 0;
        for node in &graph.nodes {
            if let Some(e) = &node.evidence
                && Self::snippet(e, sources).is_none()
            {
                invalid += 1;
            }
        }
        for edge in &graph.edges {
            let source = Self::snippet(&edge.evidence, sources);
            if source.is_none() {
                invalid += 1;
            }
            let Some(target) = nodes.get(&edge.target) else {
                invalid += 1;
                continue;
            };
            if nodes.get(&edge.source).is_none_or(|node| {
                if node.kind == NodeKind::File {
                    node.name != edge.evidence.path
                } else if node.kind == NodeKind::SyntaxUnit {
                    node.syntax_unit
                        .as_ref()
                        .filter(|id| *id == &node.id)
                        .and_then(|id| graph.syntax_units.get(id))
                        .is_none_or(|unit| {
                            !valid_units.contains(&node.id)
                                || unit.path != edge.evidence.path
                                || unit.span.start.offset > edge.evidence.span.start.offset
                                || unit.span.end.offset < edge.evidence.span.end.offset
                                || (node.id == edge.target
                                    && matches!(edge.kind, EdgeKind::Declares | EdgeKind::Contains))
                        })
                } else {
                    (node.id == edge.target
                        && matches!(edge.kind, EdgeKind::Declares | EdgeKind::Contains))
                        || node.evidence.as_ref().is_none_or(|e| {
                            e.path != edge.evidence.path
                                || e.span.start.offset > edge.evidence.span.start.offset
                                || e.span.end.offset < edge.evidence.span.end.offset
                        })
                }
            }) {
                invalid += 1;
            }
            if matches!(edge.kind, EdgeKind::Declares | EdgeKind::Contains) {
                if target.evidence.as_ref().is_none_or(|e| {
                    e.path != edge.evidence.path
                        || e.span != edge.evidence.span
                        || e.ast_id != edge.evidence.ast_id
                }) {
                    invalid += 1;
                }
                let fact = if edge.kind == EdgeKind::Declares {
                    CorpusFact::Declaration {
                        file: edge.evidence.path.clone(),
                        kind: target.kind.clone(),
                        name: target.name.clone(),
                        owner: ancestry(&edge.source),
                        source: source.unwrap_or("").into(),
                    }
                } else if let Some(region) = &target.lexical_region {
                    let text = |span| Self::span_text(&edge.evidence.path, span, sources);
                    let opener = text(region.opener);
                    let header = region.header.and_then(text);
                    let captures = region
                        .captures
                        .iter()
                        .map(|(name, span)| {
                            text(*span).map(|value| (name.clone(), value.to_owned()))
                        })
                        .collect::<Option<BTreeMap<_, _>>>();
                    let content = text(region.content_span);
                    let closer = region.closer.and_then(text);
                    let lookahead = region.lookahead_span.and_then(text);
                    if region.id != target.id
                        || region.span != edge.evidence.span
                        || region.header.is_some() != header.is_some()
                        || captures.is_none()
                        || region.header.is_some_and(|h| {
                            h.start != region.span.start
                                || h.end
                                    != if region.self_closed {
                                        region.span.end
                                    } else {
                                        region.opener.end
                                    }
                        })
                        || (region.self_closed
                            && (region.header.is_none()
                                || !region.closed
                                || region.content_span.start != region.content_span.end))
                        || region.captures.values().any(|s| {
                            s.start.offset < region.opener.start.offset
                                || s.end.offset > region.opener.end.offset
                        })
                        || opener.is_none()
                        || content.is_none()
                        || region.lookahead_span.is_some() != lookahead.is_some()
                        || region.lookahead_span.is_some_and(|s| {
                            !region.closed
                                || region.closer.is_some()
                                || s.start != region.span.end
                                || s.end.offset <= s.start.offset
                                || s.end.offset - s.start.offset > 128
                        })
                        || region.closer.is_some() != closer.is_some()
                        || region.opener.start != region.span.start
                        || region.opener.end != region.content_span.start
                        || region.content_span.end
                            != region.closer.map_or(region.span.end, |s| s.start)
                        || region.closer.is_some_and(|s| s.end != region.span.end)
                        || (!region.closed && region.closer.is_some())
                        || region.parent.as_ref().is_some_and(|id| id != &edge.source)
                    {
                        invalid += 1;
                    }
                    CorpusFact::LexicalRegion {
                        file: edge.evidence.path.clone(),
                        mode: region.mode.clone(),
                        closed: region.closed,
                        opener: opener.unwrap_or("").into(),
                        header: header.map(str::to_owned),
                        self_closed: region.self_closed,
                        captures: captures.unwrap_or_default(),
                        closer: closer.map(str::to_owned),
                        lookahead: lookahead.map(str::to_owned),
                        content: content.unwrap_or("").into(),
                        owner: ancestry(&edge.source),
                        source: source.unwrap_or("").into(),
                    }
                } else if let Some(shape) = &target.expression {
                    let operands = shape
                        .operands
                        .iter()
                        .map(|id| {
                            nodes
                                .get(id)
                                .and_then(|n| n.evidence.as_ref())
                                .and_then(|e| Self::snippet(e, sources))
                                .map(str::to_owned)
                        })
                        .collect::<Option<Vec<_>>>();
                    if operands.is_none() {
                        invalid += 1;
                    }
                    CorpusFact::Expression {
                        file: edge.evidence.path.clone(),
                        kind: shape.kind,
                        operator: shape.operator.clone(),
                        operands: operands.unwrap_or_default(),
                        owner: ancestry(&edge.source),
                        source: source.unwrap_or("").into(),
                    }
                } else {
                    CorpusFact::Container {
                        file: edge.evidence.path.clone(),
                        kind: target.kind.clone(),
                        name: target.name.clone(),
                        owner: ancestry(&edge.source),
                        source: source.unwrap_or("").into(),
                    }
                };
                facts.push(fact);
            } else if matches!(
                edge.kind,
                EdgeKind::Imports
                    | EdgeKind::DependsOn
                    | EdgeKind::Exports
                    | EdgeKind::References
                    | EdgeKind::Calls
            ) {
                facts.push(CorpusFact::Relation {
                    file: edge.evidence.path.clone(),
                    kind: edge.kind.clone(),
                    target: target.name.clone(),
                    owner: ancestry(&edge.source),
                    target_owner: if matches!(
                        edge.kind,
                        EdgeKind::Exports | EdgeKind::References | EdgeKind::Calls
                    ) && edge.resolution == nexora_graph::ResolutionState::Resolved
                    {
                        parents
                            .get(&target.id)
                            .map_or_else(Vec::new, |p| ancestry(p))
                    } else {
                        Vec::new()
                    },
                    resolution: edge.resolution.clone(),
                    resolution_reason: edge.resolution_reason.clone(),
                    label: edge.label.clone(),
                    source: source.unwrap_or("").into(),
                });
            }
        }
        (facts, invalid)
    }
}
