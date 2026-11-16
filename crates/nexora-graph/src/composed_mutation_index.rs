use crate::{CodeGraph, ComposedScopeIndex, EdgeKind, ResolutionState};
use nexora_core::{ExpressionKind as K, NexoraError, NodeId, RegionStatus};
use std::collections::{BTreeMap, BTreeSet};
/// Order-independent lexical mutation summary shared by calls and proof validation.
pub struct ComposedMutationIndex {
    pub written: BTreeSet<NodeId>,
    pub uncertain_scopes: BTreeSet<NodeId>,
}
impl ComposedMutationIndex {
    pub fn new(graph: &CodeGraph) -> Result<Self, NexoraError> {
        let fail = || NexoraError::Configuration("invalid composed mutation evidence".into());
        let nodes: BTreeMap<_, _> = graph.nodes.iter().map(|n| (&n.id, n)).collect();
        let scope_index = ComposedScopeIndex::new(graph)?;
        let scopes: BTreeMap<_, _> = scope_index
            .table
            .scopes
            .iter()
            .map(|s| (&s.id, s))
            .collect();
        let mut references = BTreeMap::new();
        for edge in graph.edges.iter().filter(|e| {
            e.kind == EdgeKind::References
                && e.scope_binding
                    .as_ref()
                    .is_some_and(|b| !b.filter_before_ambiguity)
        }) {
            if references.insert(&edge.source, edge).is_some() {
                return Err(fail());
            }
        }
        let mut written = BTreeSet::new();
        let mut unknown = BTreeSet::new();
        for scope in &scope_index.table.scopes {
            if scope
                .owner
                .as_ref()
                .and_then(|id| nodes.get(id))
                .and_then(|n| n.syntax_unit.as_ref())
                .and_then(|id| graph.syntax_units.get(id))
                .is_none_or(|u| u.status != RegionStatus::Parsed || !u.mutation_analysis)
            {
                unknown.insert(scope.id.clone());
            }
        }
        let mut budgets = BTreeMap::new();
        for node in graph.nodes.iter().filter(|n| n.mutation_operand.is_some()) {
            let scope = node.scope_id.as_ref().ok_or_else(fail)?;
            let path = node.evidence.as_ref().ok_or_else(fail)?.path.as_str();
            let remaining = budgets.entry(path).or_insert(100_000usize);
            let mut target = node
                .mutation_operand
                .as_ref()
                .and_then(|id| nodes.get(id))
                .copied();
            let mut depth = 0;
            loop {
                if *remaining == 0 {
                    return Err(NexoraError::Limit(
                        "composed mutation inspection budget".into(),
                    ));
                }
                *remaining -= 1;
                if target
                    .and_then(|n| n.expression.as_ref())
                    .is_none_or(|s| s.kind != K::Group)
                {
                    break;
                }
                if depth >= 256 {
                    unknown.insert(scope.clone());
                    target = None;
                    break;
                }
                depth += 1;
                target = target
                    .and_then(|n| n.expression.as_ref())
                    .and_then(|s| s.operands.first())
                    .and_then(|id| nodes.get(id))
                    .copied();
            }
            match target.and_then(|n| n.expression.as_ref().map(|s| (n, s.kind))) {
                Some((target, K::Identifier)) => {
                    if let Some(edge) = references
                        .get(&target.id)
                        .filter(|e| e.resolution == ResolutionState::Resolved)
                    {
                        written.insert(edge.target.clone());
                    } else {
                        unknown.insert(scope.clone());
                    }
                }
                Some((_, K::Member | K::Index)) => {}
                _ => {
                    unknown.insert(scope.clone());
                }
            }
        }
        let mut uncertain_scopes = BTreeSet::new();
        for id in unknown {
            let mut current = Some(&id);
            while let Some(id) = current {
                if !uncertain_scopes.insert(id.clone()) {
                    break;
                }
                let scope = scopes.get(id).ok_or_else(fail)?;
                current = if scope.policy.as_ref().is_some_and(|p| p.parent_lookup) {
                    scope.parent.as_ref()
                } else {
                    None
                };
            }
        }
        Ok(Self {
            written,
            uncertain_scopes,
        })
    }
}
