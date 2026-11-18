use nexora_core::{EdgeId, NodeId, SourcePosition, SourceSpan};
use nexora_graph::{
    CodeEdge, CodeGraph, CodeNode, EdgeKind, Evidence, GraphIntegrityValidator, NodeKind,
    ResolutionState,
};

#[test]
fn empty_graph_is_valid() {
    assert!(GraphIntegrityValidator::validate(&CodeGraph::default()).is_ok());
}

#[test]
fn duplicate_node_identifiers_are_rejected() {
    let duplicate = || CodeNode {
        id: NodeId("duplicate".into()),
        kind: NodeKind::File,
        name: "file.rs".into(),
        syntax_unit: None,
        lexical_region: None,
        expression: None,
        mutation_operand: None,
        expression_policy: None,
        evidence: None,
        scope_id: None,
        owned_scope_id: None,
        scope_policy: None,
        regions: Vec::new(),
        captures: Default::default(),
    };
    let graph = CodeGraph {
        nodes: vec![duplicate(), duplicate()],
        ..CodeGraph::default()
    };

    assert!(GraphIntegrityValidator::validate(&graph).is_err());
}

#[test]
fn edges_with_missing_endpoints_are_rejected() {
    let source = NodeId("source".into());
    let graph = CodeGraph {
        nodes: vec![CodeNode {
            id: source.clone(),
            kind: NodeKind::File,
            name: "file.rs".into(),
            syntax_unit: None,
            lexical_region: None,
            expression: None,
            mutation_operand: None,
            expression_policy: None,
            evidence: None,
            scope_id: None,
            owned_scope_id: None,
            scope_policy: None,
            regions: Vec::new(),
            captures: Default::default(),
        }],
        edges: vec![CodeEdge {
            id: EdgeId("edge".into()),
            source,
            target: NodeId("missing".into()),
            kind: EdgeKind::Calls,
            scope_binding: None,
            call_binding: None,
            evidence: Evidence {
                path: "file.rs".into(),
                span: SourceSpan {
                    start: SourcePosition::default(),
                    end: SourcePosition::default(),
                },
                ast_id: NodeId("ast:call".into()),
                syntax_unit: None,
                rule_version: "1".into(),
            },
            resolution: ResolutionState::Unresolved,
            resolution_reason: None,
            label: None,
        }],
        ..CodeGraph::default()
    };

    assert!(GraphIntegrityValidator::validate(&graph).is_err());
}
