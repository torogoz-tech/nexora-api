use crate::{CodeGraph, EdgeKind, NodeKind, ResolutionState, ScopeBinding};
use nexora_core::{NexoraError, NodeId, RegionStatus, SourceSpan, SymbolId};
use nexora_symbols::{Scope, Symbol, SymbolLookup, SymbolReference, SymbolTable};
use std::collections::{BTreeMap, BTreeSet};
/// Immutable lexical index for the unified graph. Incomplete units are barriers.
pub struct ComposedScopeIndex {
    pub table: SymbolTable,
    pub unavailable_scopes: BTreeSet<NodeId>,
    kinds: BTreeMap<NodeId, NodeKind>,
}
impl ComposedScopeIndex {
    pub fn new(graph: &CodeGraph) -> Result<Self, NexoraError> {
        let fail = || NexoraError::Configuration("invalid composed scope topology".into());
        let nodes: BTreeMap<_, _> = graph.nodes.iter().map(|n| (&n.id, n)).collect();
        let mut table = SymbolTable {
            symbols: Vec::new(),
            scopes: Vec::new(),
            complete: true,
            node_scopes: BTreeMap::new(),
        };
        let mut unavailable_scopes = BTreeSet::new();
        let mut scope_indexes = BTreeMap::new();
        for node in &graph.nodes {
            if let Some(id) = &node.owned_scope_id {
                if scope_indexes
                    .insert(id.clone(), table.scopes.len())
                    .is_some()
                {
                    return Err(fail());
                }
                table.scopes.push(Scope {
                    id: id.clone(),
                    parent: node.scope_id.clone(),
                    owner: Some(node.id.clone()),
                    policy: node.scope_policy.clone(),
                });
                if node
                    .syntax_unit
                    .as_ref()
                    .and_then(|id| graph.syntax_units.get(id))
                    .is_none_or(|u| u.status != RegionStatus::Parsed)
                {
                    unavailable_scopes.insert(id.clone());
                }
            }
        }
        for edge in graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::ScopeBridge)
        {
            let source = nodes.get(&edge.source).ok_or_else(fail)?;
            let target = nodes.get(&edge.target).ok_or_else(fail)?;
            let scope = target
                .owned_scope_id
                .as_ref()
                .and_then(|id| scope_indexes.get(id))
                .ok_or_else(fail)?;
            let parent = source
                .scope_id
                .as_ref()
                .filter(|id| scope_indexes.contains_key(*id))
                .ok_or_else(fail)?;
            table.scopes[*scope].parent = Some(parent.clone());
        }
        let bridged: BTreeSet<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::ScopeBridge)
            .map(|e| &e.target)
            .collect();
        for node in &graph.nodes {
            if node.kind == NodeKind::SyntaxUnit
                && node.scope_policy.as_ref().is_some_and(|p| p.parent_lookup)
                && !bridged.contains(&node.id)
                && let Some(id) = &node.owned_scope_id
            {
                unavailable_scopes.insert(id.clone());
            }
        }
        // Node scopes remain inside their own interpretation. Only a declared
        // bridge may join root scope parents across units.
        let owners: BTreeMap<_, _> = graph
            .nodes
            .iter()
            .filter_map(|n| n.owned_scope_id.as_ref().map(|id| (id, n)))
            .collect();
        for node in &graph.nodes {
            if let Some(id) = &node.scope_id {
                let owner = owners.get(id).ok_or_else(fail)?;
                if owner.syntax_unit != node.syntax_unit {
                    return Err(fail());
                }
            }
        }
        let scopes: BTreeMap<_, _> = table.scopes.iter().map(|s| (&s.id, s)).collect();
        let mut finished = BTreeSet::new();
        for scope in &table.scopes {
            let mut current = Some(&scope.id);
            let mut path = BTreeSet::new();
            while let Some(id) = current {
                if finished.contains(id) {
                    break;
                }
                if !path.insert(id.clone()) {
                    return Err(fail());
                }
                current = scopes.get(id).ok_or_else(fail)?.parent.as_ref();
            }
            finished.extend(path);
        }
        let mut declared = BTreeSet::new();
        for edge in graph.edges.iter().filter(|e| e.kind == EdgeKind::Declares) {
            let node = nodes.get(&edge.target).ok_or_else(fail)?;
            if declared.insert(node.id.clone()) {
                let scope_id = node.scope_id.clone().ok_or_else(fail)?;
                let span = node.evidence.as_ref().ok_or_else(fail)?.span;
                table.symbols.push(Symbol {
                    id: SymbolId(node.id.0.clone()),
                    ast_id: node.id.clone(),
                    name: node.name.clone(),
                    scope_id,
                    span,
                });
            }
        }
        Ok(Self {
            table,
            unavailable_scopes,
            kinds: graph
                .nodes
                .iter()
                .map(|n| (n.id.clone(), n.kind.clone()))
                .collect(),
        })
    }
    pub fn lookup(&self) -> SymbolLookup<'_> {
        SymbolLookup::with_unavailable_scopes(&self.table, &self.unavailable_scopes)
    }
    pub fn resolve(
        &self,
        lookup: &SymbolLookup<'_>,
        binding: &ScopeBinding,
        span: SourceSpan,
        remaining: &mut usize,
    ) -> (ResolutionState, Option<NodeId>) {
        let Some(candidates) = lookup.candidates(
            &SymbolReference {
                name: binding.name.clone(),
                scope_id: binding.scope_id.clone(),
                span,
            },
            remaining,
        ) else {
            return (ResolutionState::Unresolved, None);
        };
        let allowed = |symbol: &&Symbol| {
            self.kinds
                .get(&symbol.ast_id)
                .is_some_and(|kind| binding.target_kinds.contains(kind))
        };
        let candidates: Vec<_> = if binding.filter_before_ambiguity {
            candidates.iter().copied().filter(|s| allowed(s)).collect()
        } else {
            candidates
        };
        match candidates.as_slice() {
            [symbol] if allowed(symbol) => (ResolutionState::Resolved, Some(symbol.ast_id.clone())),
            [] | [_] => (ResolutionState::Unresolved, None),
            _ => (ResolutionState::Ambiguous, None),
        }
    }
}
