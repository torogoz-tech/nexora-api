use nexora_ast::{AbstractSyntaxNode, AbstractSyntaxTree};
use nexora_core::{NodeId, SourcePosition, SourceSpan};
use nexora_symbols::{ResolutionResult, SymbolReference, SymbolTable};
#[test]
fn file_scope_distinguishes_resolved_ambiguous_and_missing() {
    let span = SourceSpan {
        start: SourcePosition::default(),
        end: SourcePosition::default(),
    };
    let mut ast = AbstractSyntaxTree {
        complete: true,
        hierarchical: false,
        file_scope: None,
        nodes: vec![AbstractSyntaxNode {
            parent_id: None,
            scope: None,
            captures: Default::default(),
            regions: Vec::new(),
            id: NodeId("ast:0".into()),
            cst_id: NodeId("cst:0".into()),
            kind: "function".into(),
            name: "greet".into(),
            expression: None,
            lexical_region: None,
            role: Default::default(),
            span,
        }],
    };
    let reference = SymbolReference {
        name: "greet".into(),
        scope_id: NodeId("scope:file".into()),
        span,
    };
    assert!(matches!(
        SymbolTable::from_ast(&ast).resolve(&reference),
        ResolutionResult::Resolved(_)
    ));
    let mut duplicate = ast.nodes[0].clone();
    duplicate.id = NodeId("ast:20".into());
    duplicate.span.start.offset = 20;
    ast.nodes.push(duplicate);
    assert!(
        matches!(SymbolTable::from_ast(&ast).resolve(&reference),ResolutionResult::Ambiguous(ids) if ids.len()==2)
    );
    let missing = SymbolReference {
        name: "missing".into(),
        ..reference
    };
    assert_eq!(
        SymbolTable::from_ast(&ast).resolve(&missing),
        ResolutionResult::Unresolved
    );
}
