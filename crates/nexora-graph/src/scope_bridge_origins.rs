use crate::{CodeEdge, CodeGraph, EdgeKind};
use nexora_core::NodeId;
use std::collections::{BTreeMap, BTreeSet};
/// Picks a lexical parent independently of interpretation discovery order.
pub struct ScopeBridgeOrigins;
impl ScopeBridgeOrigins {
    pub fn select(graph: &CodeGraph) -> Vec<CodeEdge> {
        let nodes: BTreeMap<_, _> = graph.nodes.iter().map(|n| (&n.id, n)).collect();
        let mut incoming: BTreeMap<&NodeId, Vec<(&CodeEdge, usize)>> = BTreeMap::new();
        for edge in graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Interprets)
        {
            let Some(target) = nodes.get(&edge.target) else {
                continue;
            };
            if !target
                .scope_policy
                .as_ref()
                .is_some_and(|p| p.parent_lookup)
            {
                continue;
            }
            let Some(source) = nodes.get(&edge.source) else {
                continue;
            };
            let Some(origin) = source
                .syntax_unit
                .as_ref()
                .and_then(|id| graph.syntax_units.get(id))
            else {
                continue;
            };
            if source.scope_id.is_some() {
                incoming.entry(&edge.target).or_default().push((
                    edge,
                    origin
                        .span
                        .end
                        .offset
                        .saturating_sub(origin.span.start.offset),
                ));
            }
        }
        let mut selected = Vec::new();
        for candidates in incoming.values() {
            let Some(shortest) = candidates.iter().map(|(_, length)| *length).min() else {
                continue;
            };
            let nearest: Vec<_> = candidates
                .iter()
                .filter(|(_, length)| *length == shortest)
                .map(|(e, _)| *e)
                .collect();
            let parents: BTreeSet<_> = nearest
                .iter()
                .map(|e| (&nodes[&e.source].syntax_unit, &nodes[&e.source].scope_id))
                .collect();
            if parents.len() == 1
                && let Some(edge) = nearest.iter().min_by_key(|e| &e.id)
            {
                selected.push((*edge).clone());
            }
        }
        selected
    }
}
