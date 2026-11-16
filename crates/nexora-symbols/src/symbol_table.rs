use crate::{ResolutionResult, Scope, Symbol, SymbolReference};
use nexora_ast::AbstractSyntaxTree;
use nexora_core::{NodeId, SymbolId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolTable {
    pub symbols: Vec<Symbol>,
    pub scopes: Vec<Scope>,
    #[serde(default)]
    pub complete: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub node_scopes: BTreeMap<NodeId, NodeId>,
}
impl SymbolTable {
    pub fn owned_scope(id: &NodeId) -> NodeId {
        NodeId(format!("scope:{}", id.0))
    }
    pub fn from_ast(ast: &AbstractSyntaxTree) -> Self {
        let root = NodeId("scope:file".into());
        let mut table = Self {
            symbols: Vec::new(),
            scopes: vec![Scope {
                id: root.clone(),
                parent: None,
                owner: None,
                policy: ast.file_scope.clone(),
            }],
            complete: ast.complete
                && !ast.nodes.iter().any(|n| {
                    n.regions
                        .iter()
                        .any(|r| r.status != nexora_core::RegionStatus::Parsed)
                }),
            node_scopes: BTreeMap::new(),
        };
        let nodes: BTreeMap<_, _> = ast.nodes.iter().map(|n| (&n.id, n)).collect();
        for node in &ast.nodes {
            let mut scope = root.clone();
            let mut ancestor = node.parent_id.as_ref();
            let mut visited = BTreeSet::new();
            while let Some(id) = ancestor {
                if !visited.insert(id) || visited.len() > 256 {
                    table.complete = false;
                    break;
                }
                let Some(parent) = nodes.get(id) else {
                    table.complete = false;
                    break;
                };
                if parent.scope.is_some() {
                    scope = Self::owned_scope(id);
                    break;
                }
                ancestor = parent.parent_id.as_ref();
            }
            if ast.hierarchical {
                table.node_scopes.insert(node.id.clone(), scope.clone());
            }
            if let Some(policy) = &node.scope {
                table.scopes.push(Scope {
                    id: Self::owned_scope(&node.id),
                    parent: Some(scope.clone()),
                    owner: Some(node.id.clone()),
                    policy: Some(policy.clone()),
                });
            }
            if node.role == nexora_core::SyntaxRole::Declaration {
                table.symbols.push(Symbol {
                    id: SymbolId(format!(
                        "symbol:{}",
                        node.id.0.strip_prefix("ast:").unwrap_or(&node.id.0)
                    )),
                    name: node.name.clone(),
                    scope_id: scope,
                    ast_id: node.id.clone(),
                    span: node.span,
                });
            }
        }
        table
    }
    pub fn candidates(&self, reference: &SymbolReference) -> Vec<&Symbol> {
        crate::SymbolLookup::new(self)
            .candidates(reference, &mut 100_000)
            .unwrap_or_default()
    }
    pub fn resolve(&self, reference: &SymbolReference) -> ResolutionResult {
        let candidates: Vec<_> = self
            .candidates(reference)
            .iter()
            .map(|s| s.id.clone())
            .collect();
        match candidates.as_slice() {
            [] => ResolutionResult::Unresolved,
            [id] => ResolutionResult::Resolved(id.clone()),
            _ => ResolutionResult::Ambiguous(candidates),
        }
    }
}
