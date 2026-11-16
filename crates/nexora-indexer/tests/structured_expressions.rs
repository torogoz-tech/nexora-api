use nexora_ast::{AbstractSyntaxNode, AbstractSyntaxTree};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use std::{fs, path::Path};
fn language() -> LanguageDefinition {
    LanguageDefinition::load(Path::new("../../templates/semantic-molds/brace_mold")).unwrap()
}
fn analyze(l: LanguageDefinition, source: &str, name: &str) -> nexora_indexer::AnalysisResult {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-expr-{}-{name}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let ext = &l.manifest.extensions[0];
    let path = root.join(format!("source.{ext}"));
    fs::write(&path, source).unwrap();
    let result = Indexer::new(l).analyze(&path).unwrap();
    fs::remove_dir_all(root).unwrap();
    result
}
fn expression<'a>(ast: &'a AbstractSyntaxTree, source: &str, text: &str) -> &'a AbstractSyntaxNode {
    ast.nodes
        .iter()
        .find(|n| {
            serde_json::to_value(n).unwrap().get("expression").is_some()
                && &source[n.span.start.offset..n.span.end.offset] == text
        })
        .unwrap()
}
fn operator(n: &AbstractSyntaxNode) -> String {
    serde_json::to_value(n).unwrap()["expression"]["operator"]
        .as_str()
        .unwrap()
        .into()
}
fn operands(n: &AbstractSyntaxNode) -> Vec<String> {
    serde_json::to_value(n).unwrap()["expression"]["operands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect()
}
#[test]
fn two_syntaxes_build_the_same_precedence_shape_and_do_not_invent_calls() {
    for (id, ext, text, root_op, child_op) in [
        ("brace_mold", "nxb", "p + 2 * 3", "+", "*"),
        ("keyword_mold", "nxk", "p ADD 2 TIMES 3", "ADD", "TIMES"),
    ] {
        let dir = Path::new("../../templates/semantic-molds").join(id);
        let l = LanguageDefinition::load(&dir).unwrap();
        let result = Indexer::new(l)
            .analyze(&dir.join(format!("expressions.{ext}")))
            .unwrap();
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let root = expression(&result.ast, &result.source.content, text);
        assert_eq!(operator(root), root_op);
        let ids = operands(root);
        assert_eq!(ids.len(), 2);
        let right = result.ast.nodes.iter().find(|n| n.id.0 == ids[1]).unwrap();
        assert_eq!(operator(right), child_op);
        assert_eq!(right.parent_id.as_ref(), Some(&root.id));
        assert!(
            !result
                .graph
                .edges
                .iter()
                .any(|e| e.kind == nexora_graph::EdgeKind::Calls)
        );
        assert_eq!(result.symbols.symbols.len(), 3);
    }
}
#[test]
fn precedence_changes_in_data_and_parentheses_override_it() {
    let mut l = language();
    let mut g = serde_json::to_value(&l.grammar).unwrap();
    g["contexts"]["expression"]["expression"]["infix"][0]["precedence"] = serde_json::json!(50);
    l.grammar = serde_json::from_value(g).unwrap();
    let altered = analyze(l, "VAR x = a + b * c;", "precedence");
    assert!(altered.diagnostics.is_empty());
    assert_eq!(
        operator(expression(
            &altered.ast,
            &altered.source.content,
            "a + b * c"
        )),
        "*"
    );
    let grouped = analyze(language(), "VAR x = (a + b) * c;", "group");
    assert!(grouped.diagnostics.is_empty());
    let root = expression(&grouped.ast, &grouped.source.content, "(a + b) * c");
    assert_eq!(operator(root), "*");
    let left = &operands(root)[0];
    let group = grouped.ast.nodes.iter().find(|n| &n.id.0 == left).unwrap();
    assert_eq!(
        serde_json::to_value(group).unwrap()["expression"]["kind"],
        "group"
    );
}
#[test]
fn associativity_assignment_and_non_associative_operators_are_explicit() {
    let right = analyze(language(), "EVAL a = b = c;", "right");
    assert!(right.diagnostics.is_empty());
    let root = expression(&right.ast, &right.source.content, "a = b = c");
    assert_eq!(
        serde_json::to_value(root).unwrap()["expression"]["kind"],
        "assignment"
    );
    let rhs = operands(root)[1].clone();
    assert_eq!(
        operator(right.ast.nodes.iter().find(|n| n.id.0 == rhs).unwrap()),
        "="
    );
    let left = analyze(language(), "EVAL a + b + c;", "left");
    assert!(left.diagnostics.is_empty());
    let root = expression(&left.ast, &left.source.content, "a + b + c");
    let lhs = operands(root)[0].clone();
    assert_eq!(
        operator(left.ast.nodes.iter().find(|n| n.id.0 == lhs).unwrap()),
        "+"
    );
    let bad = analyze(language(), "EVAL a == b == c;", "nonassoc");
    assert!(
        bad.diagnostics
            .iter()
            .any(|d| d.message.contains("non-associative"))
    );
    let good = analyze(language(), "EVAL a == (b == c);", "explicit-nonassoc");
    assert!(good.diagnostics.is_empty());
}
#[test]
fn unary_member_and_application_keep_order_without_resolved_calls() {
    let source = "EVAL -obj.run(1, other(2 + 3))++;";
    let r = analyze(language(), source, "composed");
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    let root = expression(&r.ast, source, "-obj.run(1, other(2 + 3))++");
    assert_eq!(
        serde_json::to_value(root).unwrap()["expression"]["kind"],
        "prefix"
    );
    let applications: Vec<_> = r
        .ast
        .nodes
        .iter()
        .filter(|n| serde_json::to_value(n).unwrap()["expression"]["kind"] == "application")
        .collect();
    assert_eq!(applications.len(), 2);
    let outer = expression(&r.ast, source, "obj.run(1, other(2 + 3))");
    let ids = operands(outer);
    assert_eq!(ids.len(), 3);
    let arguments: Vec<_> = ids
        .iter()
        .map(|id| {
            let n = r.ast.nodes.iter().find(|n| &n.id.0 == id).unwrap();
            &source[n.span.start.offset..n.span.end.offset]
        })
        .collect();
    assert_eq!(arguments, vec!["obj.run", "1", "other(2 + 3)"]);
    assert!(
        !r.graph
            .edges
            .iter()
            .any(|e| e.kind == nexora_graph::EdgeKind::Calls)
    );
    assert!(r.symbols.symbols.is_empty());
    let zero = analyze(language(), "EVAL f();", "zero-args");
    assert!(zero.diagnostics.is_empty());
    assert_eq!(
        operands(expression(&zero.ast, &zero.source.content, "f()")).len(),
        1
    );
}
#[test]
fn malformed_expressions_leave_partial_regions_and_do_not_hide_siblings() {
    for (i, source) in [
        "VAR x = ;",
        "VAR x = a + ;",
        "EVAL f(a,,b);",
        "EVAL f(a,);",
        "EVAL obj.2;",
        "EVAL (a + b;",
        "EVAL 1 = 2;",
        "EVAL (a) = 2;",
        "EVAL a b;",
    ]
    .iter()
    .enumerate()
    {
        let r = analyze(language(), source, &format!("bad-{i}"));
        assert!(!r.diagnostics.is_empty(), "{source}");
        assert!(!r.ast.complete);
    }
    let source = "FUNC outer() { EVAL a + ; VAR sibling; EVAL 2 * 3; }";
    let r = analyze(language(), source, "sibling");
    assert!(!r.diagnostics.is_empty());
    assert!(r.ast.nodes.iter().any(|n| n.name == "sibling"));
    assert_eq!(operator(expression(&r.ast, source, "2 * 3")), "*");
    assert!(!r.ast.nodes.iter().any(|n| n.name == "a"));
    assert!(
        r.graph
            .syntax_coverage
            .values()
            .any(|c| c.partial_regions > 0)
    );
}
#[test]
fn literals_comments_unicode_and_operand_evidence_are_preserved() {
    let source =
        "FUNC café(p) {\r\n VAR result = p + \"a; b\"; // extra ; here\r\n EVAL result;\r\n}";
    let r = analyze(language(), source, "positions");
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    assert_eq!(operator(expression(&r.ast, source, "p + \"a; b\"")), "+");
    for n in &r.ast.nodes {
        let mut start = nexora_core::SourcePosition::default();
        start.advance(&source[..n.span.start.offset]);
        assert_eq!(start, n.span.start);
        let mut end = start;
        end.advance(&source[n.span.start.offset..n.span.end.offset]);
        assert_eq!(end, n.span.end);
    }
    let graph_ids: std::collections::BTreeSet<_> = r.graph.nodes.iter().map(|n| &n.id).collect();
    assert!(
        r.graph
            .nodes
            .iter()
            .filter_map(|n| n.expression.as_ref())
            .flat_map(|e| e.operands.iter())
            .all(|id| graph_ids.contains(id))
    );
}
#[test]
fn expression_steps_and_tree_height_are_bounded() {
    let mut l = language();
    l.grammar.max_steps = 90;
    let r = analyze(l, "VAR x = 1 + 2 + 3 + 4 + 5 + 6;", "steps");
    assert!(
        r.diagnostics.iter().any(|d| d.message.contains("budget")),
        "{:?}",
        r.diagnostics
    );
    assert!(r.ast.nodes.iter().any(|n| n.name == "x"));
    assert!(
        r.graph
            .syntax_coverage
            .values()
            .any(|c| c.partial_regions > 0)
    );
    let mut l = language();
    l.grammar.max_nesting = 32;
    let source = format!("EVAL {};", vec!["1"; 40].join(" + "));
    let r = analyze(l, &source, "height");
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains("height budget")),
        "{:?}",
        r.diagnostics
    );
}
#[test]
fn conflicting_or_unversioned_expression_tables_are_rejected() {
    let original = serde_json::to_value(&language().grammar).unwrap();
    for mode in [
        "schema",
        "duplicate",
        "boundary",
        "empty-targets",
        "precedence",
        "bad-kind",
        "context-both",
        "unknown-atoms",
    ] {
        let mut value = original.clone();
        let expression = &mut value["contexts"]["expression"]["expression"];
        match mode {
            "schema" => value["schema_version"] = serde_json::json!(3),
            "duplicate" => {
                let duplicate = expression["infix"][0].clone();
                expression["infix"].as_array_mut().unwrap().push(duplicate);
            }
            "boundary" => {
                expression["application"][0]["separator"] = serde_json::json!({"text":"+"})
            }
            "empty-targets" => expression["assignment_targets"] = serde_json::json!([]),
            "precedence" => expression["infix"][0]["precedence"] = serde_json::json!(65535),
            "bad-kind" => expression["infix"][0]["kind"] = serde_json::json!("literal"),
            "context-both" => {
                value["contexts"]["expression"]["productions"] = serde_json::json!(["variable"])
            }
            _ => expression["literals"] = serde_json::json!(["identifier"]),
        }
        let mut l = language();
        l.grammar = serde_json::from_value(value).unwrap();
        assert!(l.validate().is_err(), "{mode}");
    }
}
#[test]
fn expression_regions_share_the_file_diagnostic_limit() {
    let source = format!("FUNC f() {{ {} }}", "EVAL a +;".repeat(1100));
    let r = analyze(language(), &source, "diagnostics");
    assert!(
        r.diagnostics.len() <= 1025,
        "{} diagnostics",
        r.diagnostics.len()
    );
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains("diagnostic budget"))
    );
}
#[test]
fn declarations_cannot_claim_the_generated_expression_node_kind() {
    let mut l = language();
    l.graph
        .mappings
        .insert("variable".into(), "Expression".into());
    assert!(l.validate().is_err());
}
#[test]
fn incremental_snapshots_preserve_shapes_and_match_full_analysis_after_edits() {
    use nexora_core::TenantId;
    use nexora_indexer::{IndexCache, MemoryStore};
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-expr-cache-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("a.nxb"),
        "FUNC f(p) { VAR x = p + 2 * 3; EVAL x; }",
    )
    .unwrap();
    fs::write(root.join("b.nxb"), "EVAL obj.run(1, 2);").unwrap();
    let indexer = Indexer::new(language());
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let cold = indexer
        .index_incremental(&root, TenantId("test".into()), &mut store, &mut cache)
        .unwrap();
    let warm = indexer
        .index_incremental(&root, TenantId("test".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 2);
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    assert!(
        warm.snapshot
            .graph
            .nodes
            .iter()
            .any(|n| n.expression.as_ref().is_some_and(|e| e.operands.len() == 3))
    );
    fs::write(
        root.join("a.nxb"),
        "FUNC f(p) { VAR x = (p + 2) * 3; EVAL x; }",
    )
    .unwrap();
    let changed = indexer
        .index_incremental(&root, TenantId("test".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(changed.reuse.reused_files, 1);
    let full = indexer
        .index(&root, TenantId("test".into()), &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&changed.snapshot).unwrap(),
        serde_json::to_value(&full).unwrap()
    );
    assert_ne!(changed.snapshot.id, cold.snapshot.id);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn mixed_associativities_at_the_same_precedence_require_an_unambiguous_table() {
    let mut l = language();
    let e = l
        .grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap();
    let non_associative = e
        .infix
        .iter()
        .find(|o| o.associativity == nexora_language::OperatorAssociativity::NonAssociative)
        .unwrap()
        .precedence;
    e.infix
        .iter_mut()
        .find(|o| o.token.text == "^")
        .unwrap()
        .precedence = non_associative;
    assert!(l.validate().is_err());
}
#[test]
fn scanner_delimiters_cannot_be_operators_or_argument_separators() {
    for (field, token) in [
        ("application", "{"),
        ("application", "}"),
        ("prefix", "{"),
        ("infix", "{"),
    ] {
        let mut l = language();
        let mut grammar = serde_json::to_value(&l.grammar).unwrap();
        let entry = &mut grammar["contexts"]["expression"]["expression"][field][0];
        entry[if field == "application" {
            "separator"
        } else {
            "token"
        }] = serde_json::json!({"text": token, "kind": "delimiter"});
        l.grammar = serde_json::from_value(grammar).unwrap();
        assert!(l.validate().is_err(), "{field}: {token}");
    }
}
