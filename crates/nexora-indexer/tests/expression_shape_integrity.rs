use nexora_graph::{CodeGraph, GraphIntegrityValidator};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use serde_json::json;
use std::{fs, path::Path};

fn graph(source: &str, label: &str) -> CodeGraph {
    let mut l =
        LanguageDefinition::load(Path::new("../../templates/semantic-molds/brace_mold")).unwrap();
    l.lexer
        .tokens
        .iter_mut()
        .find(|t| t.kind == "operator")
        .unwrap()
        .pattern
        .push_str("|[?:]");
    let mut g = serde_json::to_value(&l.grammar).unwrap();
    g["schema_version"] = json!(9);
    g["contexts"]["expression"]["expression"]["conditional"] = json!([{"token":{"text":"?","kind":"operator"},"separator":{"text":":","kind":"operator"},"precedence":15,"consequent_precedence":10,"alternative_precedence":10}]);
    l.grammar = serde_json::from_value(g).unwrap();
    let mut g = serde_json::to_value(&l.graph).unwrap();
    g["schema_version"] = json!(5);
    g["expression_bindings"] = json!({"expression":{"target_kinds":["Function","Variable","Parameter"],"callable_kinds":["Function"],"unwrap_groups":true,"mutating_unary_operators":["++"]}});
    l.graph = serde_json::from_value(g).unwrap();
    let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "nexora-shape-integrity-{}-{label}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("public.nxb");
    fs::write(&path, source).unwrap();
    let r = Indexer::new(l).analyze(&path).unwrap();
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    fs::remove_dir_all(dir).unwrap();
    r.graph
}
fn conditional(g: &CodeGraph) -> usize {
    g.nodes
        .iter()
        .position(|n| {
            n.expression
                .as_ref()
                .is_some_and(|s| s.kind == nexora_core::ExpressionKind::Conditional)
        })
        .unwrap()
}
#[test]
fn invalid_conditional_arity_and_missing_or_shared_operands_are_rejected() {
    let original = graph(
        "VAR flag; VAR a; VAR b; VAR value=flag ? a : b;",
        "references",
    );
    GraphIntegrityValidator::validate(&original).unwrap();
    for change in ["arity", "missing", "shared", "operator"] {
        let mut altered = original.clone();
        let index = conditional(&altered);
        let shape = altered.nodes[index].expression.as_mut().unwrap();
        match change {
            "arity" => {
                shape.operands.pop();
            }
            "missing" => shape.operands[2] = nexora_core::NodeId("absent-expression".into()),
            "shared" => shape.operands[2] = shape.operands[1].clone(),
            "operator" => shape.operator = None,
            _ => unreachable!(),
        }
        assert!(
            GraphIntegrityValidator::validate(&altered).is_err(),
            "accepted {change}"
        );
    }
}
#[test]
fn cyclic_expression_operands_and_outside_ranges_are_rejected() {
    let original = graph("VAR flag; VAR a; VAR b; VAR value=flag ? a : b;", "cycles");
    let index = conditional(&original);
    let root = original.nodes[index].id.clone();
    let operand = original.nodes[index].expression.as_ref().unwrap().operands[0].clone();
    let mut cyclic = original.clone();
    for node in &mut cyclic.nodes {
        if node.expression.is_some() {
            node.evidence = None;
        }
    }
    let child = cyclic.nodes.iter_mut().find(|n| n.id == operand).unwrap();
    child.expression = Some(nexora_core::ExpressionShape {
        kind: nexora_core::ExpressionKind::Group,
        operator: Some("(".into()),
        operands: vec![root],
    });
    let mut link = cyclic
        .edges
        .iter()
        .find(|e| {
            e.kind == nexora_graph::EdgeKind::Contains && e.target == original.nodes[index].id
        })
        .unwrap()
        .clone();
    cyclic.edges.retain(|e| {
        !(e.kind == nexora_graph::EdgeKind::Contains && e.target == original.nodes[index].id)
    });
    link.id = nexora_core::EdgeId("cyclic-expression-parent".into());
    link.source = operand.clone();
    cyclic.edges.push(link);
    assert!(GraphIntegrityValidator::validate(&cyclic).is_err());
    let mut ranged = original.clone();
    let span = ranged.nodes[index].evidence.as_ref().unwrap().span;
    ranged
        .nodes
        .iter_mut()
        .find(|n| n.id == operand)
        .unwrap()
        .evidence
        .as_mut()
        .unwrap()
        .span = span;
    assert!(GraphIntegrityValidator::validate(&ranged).is_err());
}
#[test]
fn recursive_call_relations_do_not_make_expression_operands_cyclic() {
    let original = graph(
        "FUNC recurse() { EVAL recurse(); } VAR flag; VAR a; VAR b; VAR value=flag ? a : b;",
        "recursive",
    );
    assert!(
        original
            .edges
            .iter()
            .any(|e| e.kind == nexora_graph::EdgeKind::Calls
                && e.resolution == nexora_graph::ResolutionState::Resolved)
    );
    let roundtrip: CodeGraph =
        serde_json::from_value(serde_json::to_value(original).unwrap()).unwrap();
    GraphIntegrityValidator::validate(&roundtrip).unwrap();
}

#[test]
fn operand_links_and_contains_edges_cannot_disagree() {
    let original = graph(
        "VAR flag; VAR a; VAR b; VAR value=flag ? a : b;",
        "contains",
    );
    let root = conditional(&original);
    let operand = original.nodes[root].expression.as_ref().unwrap().operands[1].clone();
    let mut altered = original.clone();
    altered
        .edges
        .retain(|e| !(e.kind == nexora_graph::EdgeKind::Contains && e.target == operand));
    assert!(GraphIntegrityValidator::validate(&altered).is_err());
}
