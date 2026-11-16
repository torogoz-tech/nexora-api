use crate::{
    ConcreteSyntaxNode, ConcreteSyntaxTree, ParseDiagnostic, lexical_owner_index::LexicalOwnerIndex,
};
use nexora_core::{NodeId, RegionStatus, SyntaxRegion, SyntaxRole};
use nexora_lexer::Token;
pub(crate) struct LexicalRegionProjector;
impl LexicalRegionProjector {
    pub fn append(tree: &mut ConcreteSyntaxTree, tokens: &[Token]) {
        let mut owners = LexicalOwnerIndex::new(&tree.nodes);
        for (index, token) in tokens.iter().enumerate() {
            let mut root_owner = None;
            for original in &token.regions {
                if !tree.hierarchical || tree.nodes.len() >= 100_000 {
                    if tree.diagnostics.len() < 1024 {
                        tree.diagnostics.push(ParseDiagnostic {message:"lexical region projection requires hierarchy and available node budget".into(),span:original.opener});
                    }
                    return;
                }
                let cst_id = |id: &NodeId| NodeId(format!("cst:{}", id.0));
                let mut region = original.clone();
                region.id = cst_id(&original.id);
                region.parent = original.parent.as_ref().map(cst_id);
                if original.parent.is_none() {
                    root_owner = owners.owner(original.span);
                }
                if !region.closed && tree.diagnostics.len() < 1024 {
                    tree.diagnostics.push(ParseDiagnostic {
                        message: format!("unterminated lexical region {}", region.mode),
                        span: region.opener,
                    });
                }
                tree.nodes.push(ConcreteSyntaxNode {
                    id: region.id.clone(),
                    production: "lexical_region".into(),
                    ast_kind: region.ast_kind.clone(),
                    name: region.mode.clone(),
                    lexical_region: Some(region.clone()),
                    expression: None,
                    parent_id: region.parent.clone().or_else(|| root_owner.clone()),
                    scope: None,
                    captures: Default::default(),
                    regions: vec![SyntaxRegion {
                        coverage: None,
                        capture: "embedded_content".into(),
                        span: region.content_span,
                        token_start: index,
                        token_end: index + 1,
                        context: None,
                        status: if region.closed {
                            RegionStatus::Opaque
                        } else {
                            RegionStatus::Partial
                        },
                    }],
                    role: SyntaxRole::Container,
                    span: region.span,
                    token_start: index,
                    token_end: index + 1,
                });
                if let Some(header) = region.header {
                    tree.nodes
                        .last_mut()
                        .expect("projected lexical node")
                        .regions
                        .push(SyntaxRegion {
                            capture: "embedded_header".into(),
                            span: header,
                            token_start: index,
                            token_end: index + 1,
                            context: None,
                            status: RegionStatus::Opaque,
                            coverage: None,
                        });
                }
            }
        }
    }
}
