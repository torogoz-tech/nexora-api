use nexora_graph::{EdgeKind, ResolutionState};
use nexora_indexer::{AnalysisResult, Indexer};
use nexora_language::LanguageDefinition;
use serde_json::json;
use std::{fs, path::Path};

fn language(id: &str) -> LanguageDefinition {
    let mut l =
        LanguageDefinition::load(&Path::new("../../templates/semantic-molds").join(id)).unwrap();
    l.lexer
        .tokens
        .iter_mut()
        .find(|t| t.kind == "operator")
        .unwrap()
        .pattern
        .push_str("|\\?|:");
    l.lexer.keywords.extend(["WHEN".into(), "OTHER".into()]);
    let (token, separator, kind) = if id == "brace_mold" {
        ("?", ":", "operator")
    } else {
        ("WHEN", "OTHER", "keyword")
    };
    let mut grammar = serde_json::to_value(&l.grammar).unwrap();
    grammar["schema_version"] = json!(9);
    grammar["contexts"]["expression"]["expression"]["conditional"] = json!([{"token":{"text":token,"kind":kind},"separator":{"text":separator,"kind":kind},"precedence":15,"consequent_precedence":10,"alternative_precedence":10}]);
    l.grammar = serde_json::from_value(grammar).unwrap();
    let mut graph = serde_json::to_value(&l.graph).unwrap();
    graph["schema_version"] = json!(5);
    graph["expression_bindings"] = json!({"expression":{"target_kinds":["Function","Variable","Parameter"],"callable_kinds":["Function"],"unwrap_groups":true,"mutating_unary_operators":["++"]}});
    l.graph = serde_json::from_value(graph).unwrap();
    l.validate().unwrap();
    l
}
fn analyze(l: LanguageDefinition, source: &str, name: &str) -> AnalysisResult {
    let directory = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-conditional-{}-{name}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("public.{}", l.manifest.extensions[0]));
    fs::write(&path, source).unwrap();
    let result = Indexer::new(l).analyze(&path).unwrap();
    fs::remove_dir_all(directory).unwrap();
    result
}
fn expression<'a>(r: &'a AnalysisResult, text: &str) -> &'a nexora_ast::AbstractSyntaxNode {
    r.ast
        .nodes
        .iter()
        .find(|n| {
            n.expression.is_some()
                && &r.source.content[n.span.start.offset..n.span.end.offset] == text
        })
        .unwrap()
}
#[test]
fn two_syntaxes_preserve_ordered_condition_and_both_branches() {
    for (id, source, text) in [
        (
            "brace_mold",
            "VAR flag; VAR a; VAR b; VAR value=flag ? a+2 : b*3;",
            "flag ? a+2 : b*3",
        ),
        (
            "keyword_mold",
            "LET flag; LET a; LET b; LET value BECOMES flag WHEN a ADD 2 OTHER b TIMES 3;",
            "flag WHEN a ADD 2 OTHER b TIMES 3",
        ),
    ] {
        let r = analyze(language(id), source, id);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        let root = expression(&r, text);
        let shape = root.expression.as_ref().unwrap();
        assert_eq!(
            serde_json::to_value(shape.kind).unwrap(),
            json!("conditional")
        );
        assert_eq!(shape.operands.len(), 3);
        let values: Vec<_> = shape
            .operands
            .iter()
            .map(|id| {
                let n = r.ast.nodes.iter().find(|n| &n.id == id).unwrap();
                &source[n.span.start.offset..n.span.end.offset]
            })
            .collect();
        assert_eq!(values[0], "flag");
        assert!(values[1].starts_with('a'));
        assert!(values[2].starts_with('b'));
        assert!(r.graph.edges.iter().all(|e| e.kind != EdgeKind::Calls));
    }
}
#[test]
fn nested_arms_and_assignments_follow_declared_binding_levels() {
    let source = "VAR flag; VAR a; VAR b; VAR c; VAR d; VAR value=flag ? a : b ? c : d;";
    let r = analyze(language("brace_mold"), source, "right");
    assert!(r.diagnostics.is_empty());
    let root = expression(&r, "flag ? a : b ? c : d")
        .expression
        .as_ref()
        .unwrap();
    let alternative = r
        .ast
        .nodes
        .iter()
        .find(|n| n.id == root.operands[2])
        .unwrap();
    assert_eq!(
        &source[alternative.span.start.offset..alternative.span.end.offset],
        "b ? c : d"
    );
    let source = "VAR flag; VAR a; VAR b; VAR value=flag ? a=1 : b=2;";
    let r = analyze(language("brace_mold"), source, "assignments");
    assert!(r.diagnostics.is_empty());
    let root = expression(&r, "flag ? a=1 : b=2")
        .expression
        .as_ref()
        .unwrap();
    for id in &root.operands[1..] {
        assert_eq!(
            r.ast
                .nodes
                .iter()
                .find(|n| &n.id == id)
                .unwrap()
                .expression
                .as_ref()
                .unwrap()
                .kind,
            nexora_core::ExpressionKind::Assignment
        );
    }
}
#[test]
fn malformed_arms_keep_neighbors_and_older_schema_rejects_the_new_table() {
    for (name, value) in [
        ("empty", "flag ? : 1"),
        ("missing", "flag ? 1"),
        ("last", "flag ? 1 :"),
    ] {
        let source = format!("VAR flag; VAR value={value}; VAR after=1;");
        let r = analyze(language("brace_mold"), &source, name);
        assert!(!r.diagnostics.is_empty());
        assert!(
            r.graph
                .nodes
                .iter()
                .any(|n| n.kind == nexora_graph::NodeKind::Variable && n.name == "after")
        );
    }
    let mut l = language("brace_mold");
    l.grammar.schema_version = 8;
    assert!(l.validate().is_err());
}
#[test]
fn branch_calls_are_syntactic_and_potential_branch_writes_block_certainty() {
    let source = "VAR flag; FUNC first() {} FUNC second() {} EVAL flag ? first() : second(); EVAL (flag ? first : second)();";
    let r = analyze(language("brace_mold"), source, "calls");
    let calls: Vec<_> = r
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .collect();
    assert_eq!(calls.len(), 3);
    assert_eq!(
        calls
            .iter()
            .filter(|e| e.resolution == ResolutionState::Resolved)
            .count(),
        2
    );
    assert_eq!(
        calls
            .iter()
            .filter(|e| e.resolution == ResolutionState::Unresolved)
            .count(),
        1
    );
    let r = analyze(
        language("brace_mold"),
        "VAR flag; FUNC target() {} EVAL flag ? 0 : (target=1); EVAL target();",
        "writes",
    );
    assert!(
        r.graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .all(|e| e.resolution == ResolutionState::Unresolved)
    );
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message == "binding_is_mutated")
    );
}

#[test]
fn branch_minimums_are_data_and_restricted_assignment_preserves_neighbors() {
    for branch in ["consequent_precedence", "alternative_precedence"] {
        let mut l = language("brace_mold");
        let mut g = serde_json::to_value(&l.grammar).unwrap();
        g["contexts"]["expression"]["expression"]["conditional"][0][branch] = json!(16);
        l.grammar = serde_json::from_value(g).unwrap();
        let r = analyze(
            l,
            "VAR flag; VAR a; VAR b; VAR value=flag ? a=1 : b=2; VAR after=1;",
            branch,
        );
        assert!(r.diagnostics.iter().any(|d| d.stage == "parser"));
        assert!(
            r.graph
                .nodes
                .iter()
                .any(|n| n.kind == nexora_graph::NodeKind::Variable && n.name == "after")
        );
    }
}
#[test]
fn conditional_contract_rejects_ambiguous_markers_and_resource_overflow() {
    let original = language("brace_mold");
    for (label, path, value) in [
        (
            "separator",
            "separator",
            json!({"text":"+","kind":"operator"}),
        ),
        ("token", "token", json!({"text":"+","kind":"operator"})),
        ("group", "separator", json!({"text":"(","kind":"delimiter"})),
        ("kind", "token", json!({"text":"?","kind":"absent"})),
        ("precedence", "precedence", json!(0)),
        ("minimum", "alternative_precedence", json!(1025)),
    ] {
        let mut l = original.clone();
        let mut g = serde_json::to_value(&l.grammar).unwrap();
        g["contexts"]["expression"]["expression"]["conditional"][0][path] = value;
        l.grammar = serde_json::from_value(g).unwrap();
        assert!(l.validate().is_err(), "accepted {label}");
    }
    for count in [16, 17] {
        let mut l = original.clone();
        let mut g = serde_json::to_value(&l.grammar).unwrap();
        let mut operators = Vec::new();
        for index in 0..count {
            let mut c = g["contexts"]["expression"]["expression"]["conditional"][0].clone();
            c["token"]["text"] = json!(format!("?{index}"));
            operators.push(c);
        }
        g["contexts"]["expression"]["expression"]["conditional"] = json!(operators);
        l.grammar = serde_json::from_value(g).unwrap();
        assert_eq!(l.validate().is_ok(), count == 16);
    }
}

#[test]
fn literal_markers_cannot_disable_existing_literal_operands() {
    let original = language("brace_mold");
    for (field, selector) in [
        ("separator", json!({"text":"1","kind":"number"})),
        ("separator", json!({"text":"1"})),
        ("token", json!({"text":"1","kind":"number"})),
        ("separator", json!({"text":"\"x\"","kind":"string"})),
        ("separator", json!({"text":"\"x\""})),
    ] {
        let mut l = original.clone();
        let mut g = serde_json::to_value(&l.grammar).unwrap();
        g["contexts"]["expression"]["expression"]["conditional"][0][field] = selector;
        l.grammar = serde_json::from_value(g).unwrap();
        assert!(l.validate().is_err(), "accepted literal marker in {field}");
    }
    let mut l = original;
    let mut g = serde_json::to_value(&l.grammar).unwrap();
    g["contexts"]["expression"]["expression"]["conditional"][0]["token"] = json!({"text":"?"});
    g["contexts"]["expression"]["expression"]["conditional"][0]["separator"] = json!({"text":":"});
    l.grammar = serde_json::from_value(g).unwrap();
    l.validate().unwrap();
    let r = analyze(l, "VAR value=1; VAR after=2;", "safe-literals");
    assert!(r.diagnostics.is_empty());
}

#[test]
fn contextual_word_separator_does_not_capture_values_in_other_contexts() {
    let mut l = language("brace_mold");
    l.grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap()
        .conditional[0]
        .separator = nexora_language::TokenSelector {
        text: "a".into(),
        kind: Some("identifier".into()),
    };
    l.validate().unwrap();
    let source = "VAR flag; VAR a=1; VAR value=flag ? (a) a a; VAR after=a;";
    let r = analyze(l.clone(), source, "contextual");
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    assert_eq!(
        expression(&r, "flag ? (a) a a")
            .expression
            .as_ref()
            .unwrap()
            .operands
            .len(),
        3
    );
    let source = "FUNC f(p){EVAL p;} VAR flag; VAR a=1; VAR value=flag ? f(a) a a;";
    let r = analyze(l, source, "contextual-arguments");
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    assert_eq!(
        expression(&r, "flag ? f(a) a a")
            .expression
            .as_ref()
            .unwrap()
            .operands
            .len(),
        3
    );
}
