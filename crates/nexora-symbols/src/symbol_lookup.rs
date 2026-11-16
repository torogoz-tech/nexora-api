use crate::{Scope, Symbol, SymbolReference, SymbolTable};
use nexora_core::{DeclarationVisibility, NodeId, ScopePolicy, ShadowingPolicy};
use std::collections::{BTreeMap, BTreeSet};
/// Borrowed immutable index for a batch of bounded lexical lookups.
pub struct SymbolLookup<'a> {
    table: &'a SymbolTable,
    unavailable_scopes: Option<&'a BTreeSet<NodeId>>,
    scopes: BTreeMap<&'a NodeId, &'a Scope>,
    symbols: BTreeMap<(&'a NodeId, &'a str), Vec<&'a Symbol>>,
}
impl<'a> SymbolLookup<'a> {
    pub fn new(table: &'a SymbolTable) -> Self {
        Self::build(table, None)
    }
    pub fn with_unavailable_scopes(table: &'a SymbolTable, scopes: &'a BTreeSet<NodeId>) -> Self {
        Self::build(table, Some(scopes))
    }
    fn build(table: &'a SymbolTable, unavailable_scopes: Option<&'a BTreeSet<NodeId>>) -> Self {
        let mut symbols: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for symbol in &table.symbols {
            symbols
                .entry((&symbol.scope_id, symbol.name.as_str()))
                .or_default()
                .push(symbol);
        }
        Self {
            table,
            unavailable_scopes,
            scopes: table.scopes.iter().map(|s| (&s.id, s)).collect(),
            symbols,
        }
    }
    pub fn candidates(
        &self,
        reference: &SymbolReference,
        remaining: &mut usize,
    ) -> Option<Vec<&'a Symbol>> {
        if !self.table.complete {
            return None;
        }
        let mut scope = Some(reference.scope_id.clone());
        let mut visited = BTreeSet::new();
        let mut candidates = Vec::new();
        while let Some(id) = scope {
            if *remaining == 0 || !visited.insert(id.clone()) || visited.len() > 256 {
                return None;
            }
            *remaining -= 1;
            if self.unavailable_scopes.is_some_and(|s| s.contains(&id)) {
                return None;
            }
            let current = *self.scopes.get(&id)?;
            let default = ScopePolicy::default();
            let policy = current.policy.as_ref().unwrap_or(&default);
            let before = candidates.len();
            for symbol in self
                .symbols
                .get(&(&id, reference.name.as_str()))
                .into_iter()
                .flatten()
            {
                if *remaining == 0 {
                    return None;
                }
                *remaining -= 1;
                if policy.visibility == DeclarationVisibility::WholeScope
                    || symbol.span.end.offset <= reference.span.start.offset
                {
                    candidates.push(*symbol);
                }
            }
            if candidates.len() > before && policy.shadowing == ShadowingPolicy::Nearest {
                return Some(candidates);
            }
            if !policy.parent_lookup {
                break;
            }
            scope = current.parent.clone();
        }
        Some(candidates)
    }
}
