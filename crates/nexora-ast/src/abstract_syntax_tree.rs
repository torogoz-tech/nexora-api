use crate::AbstractSyntaxNode;
use nexora_core::NodeId;
use nexora_parser::ConcreteSyntaxTree;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbstractSyntaxTree {
    pub nodes: Vec<AbstractSyntaxNode>,
    #[serde(default)]
    pub complete: bool,
    #[serde(default)]
    pub hierarchical: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_scope: Option<nexora_core::ScopePolicy>,
}
impl AbstractSyntaxTree {
    fn node_id(cst: &NodeId) -> NodeId {
        NodeId(format!(
            "ast:{}",
            cst.0.strip_prefix("cst:").unwrap_or(&cst.0)
        ))
    }
    pub fn from_cst(cst: &ConcreteSyntaxTree) -> Self {
        Self {
            complete: cst.diagnostics.is_empty()
                && !cst
                    .nodes
                    .iter()
                    .flat_map(|n| &n.regions)
                    .any(|r| r.status == nexora_core::RegionStatus::Partial),
            hierarchical: cst.hierarchical,
            file_scope: cst.file_scope.clone(),
            nodes: cst
                .nodes
                .iter()
                .map(|n| AbstractSyntaxNode {
                    id: Self::node_id(&n.id),
                    parent_id: n.parent_id.as_ref().map(Self::node_id),
                    scope: n.scope.clone(),
                    captures: n.captures.clone(),
                    regions: n
                        .regions
                        .iter()
                        .cloned()
                        .map(|mut region| {
                            if let Some(proof) = &mut region.coverage {
                                proof.regions = proof.regions.iter().map(Self::node_id).collect();
                            }
                            region
                        })
                        .collect(),
                    cst_id: n.id.clone(),
                    kind: n.ast_kind.clone(),
                    name: n.name.clone(),
                    lexical_region: n.lexical_region.as_ref().map(|r| {
                        let mut r = r.clone();
                        r.id = Self::node_id(&r.id);
                        r.parent = r.parent.as_ref().map(Self::node_id);
                        r
                    }),
                    expression: n.expression.as_ref().map(|e| nexora_core::ExpressionShape {
                        kind: e.kind,
                        operator: e.operator.clone(),
                        operands: e.operands.iter().map(Self::node_id).collect(),
                    }),
                    role: n.role,
                    span: n.span,
                })
                .collect(),
        }
    }
}
