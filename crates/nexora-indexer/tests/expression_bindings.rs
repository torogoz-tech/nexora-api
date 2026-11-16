use nexora_graph::{EdgeKind, ResolutionState};
use nexora_indexer::{AnalysisResult, Indexer};
use nexora_language::LanguageDefinition;
use std::{fs, path::Path};
fn language(id: &str) -> LanguageDefinition {
    let mut l =
        LanguageDefinition::load(&Path::new("../../templates/semantic-molds").join(id)).unwrap();
    let mut graph = serde_json::to_value(&l.graph).unwrap();
    graph["schema_version"] = serde_json::json!(5);
    graph["expression_bindings"] = serde_json::json!({"expression": {"target_kinds":["Function","Variable","Parameter"],"callable_kinds":["Function"],"unwrap_groups":true,"mutating_unary_operators":["++"]}});
    l.graph = serde_json::from_value(graph).unwrap();
    l
}
fn analyze(l: LanguageDefinition, source: &str, name: &str) -> AnalysisResult {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-bind-{}-{name}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join(format!("source.{}", l.manifest.extensions[0]));
    fs::write(&path, source).unwrap();
    let result = Indexer::new(l).analyze(&path).unwrap();
    fs::remove_dir_all(root).unwrap();
    result
}
#[test]
fn both_syntaxes_bind_uses_and_direct_calls_from_expression_sites() {
    for (id, source) in [
        (
            "brace_mold",
            "FUNC target(p) { EVAL p; } FUNC outer(x) { VAR local = x; EVAL target(local); }",
        ),
        (
            "keyword_mold",
            "DEF target [p] BEGIN VALUE p; FINISH DEF outer [x] BEGIN LET local BECOMES x; VALUE target[local]; FINISH",
        ),
    ] {
        let r = analyze(language(id), source, id);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
        let refs: Vec<_> = r
            .graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::References)
            .collect();
        assert_eq!(refs.len(), 4);
        assert!(
            refs.iter()
                .all(|e| e.resolution == ResolutionState::Resolved)
        );
        let calls: Vec<_> = r
            .graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].resolution, ResolutionState::Resolved);
        let target = r
            .graph
            .nodes
            .iter()
            .find(|n| n.id == calls[0].target)
            .unwrap();
        assert_eq!(target.name, "target");
        let site = r
            .graph
            .nodes
            .iter()
            .find(|n| n.id == calls[0].source)
            .unwrap();
        assert!(site.expression.is_some());
        assert_eq!(site.evidence.as_ref().unwrap().span, calls[0].evidence.span);
    }
}
#[test]
fn nearest_non_callable_binding_shadows_a_function_before_kind_filtering() {
    let r = analyze(
        language("brace_mold"),
        "FUNC f() {} FUNC outer(f) { EVAL f(); }",
        "shadow",
    );
    let call = r
        .graph
        .edges
        .iter()
        .find(|e| e.kind == EdgeKind::Calls)
        .unwrap();
    assert_eq!(call.resolution, ResolutionState::Unresolved);
    let reference = r
        .graph
        .edges
        .iter()
        .find(|e| e.kind == EdgeKind::References)
        .unwrap();
    assert_eq!(reference.resolution, ResolutionState::Resolved);
    assert_eq!(
        r.graph
            .nodes
            .iter()
            .find(|n| n.id == reference.target)
            .unwrap()
            .kind,
        nexora_graph::NodeKind::Parameter
    );
}
fn calls(r: &AnalysisResult) -> Vec<&nexora_graph::CodeEdge> {
    r.graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .collect()
}
#[test]
fn ambiguous_missing_member_and_indirect_callees_keep_explicit_reasons() {
    for (name, source, state, reason) in [
        (
            "ambiguous",
            "FUNC f() {} FUNC f() {} EVAL f();",
            ResolutionState::Ambiguous,
            "ambiguous_lexical_binding",
        ),
        (
            "missing",
            "EVAL unknown();",
            ResolutionState::Unresolved,
            "missing_declaration",
        ),
        (
            "member",
            "FUNC run() {} VAR obj; EVAL obj.run();",
            ResolutionState::Unresolved,
            "indirect_or_member_callee",
        ),
        (
            "indirect",
            "VAR f; EVAL f();",
            ResolutionState::Unresolved,
            "binding_not_directly_callable",
        ),
        (
            "returned",
            "FUNC f() {} EVAL f()();",
            ResolutionState::Unresolved,
            "indirect_or_member_callee",
        ),
    ] {
        let r = analyze(language("brace_mold"), source, name);
        let cs = calls(&r);
        assert!(
            cs.iter().any(|e| e.resolution == state
                && serde_json::to_value(e).unwrap()["resolution_reason"] == reason),
            "{name}"
        );
        if name == "member" {
            assert_eq!(
                r.graph
                    .edges
                    .iter()
                    .filter(|e| e.kind == EdgeKind::References)
                    .count(),
                1
            );
        }
    }
}
#[test]
fn groups_are_transparent_only_when_the_template_opts_in() {
    let source = "FUNC f() {} EVAL (f)();";
    let enabled = analyze(language("brace_mold"), source, "group-on");
    assert_eq!(calls(&enabled)[0].resolution, ResolutionState::Resolved);
    let mut l = language("brace_mold");
    let mut g = serde_json::to_value(&l.graph).unwrap();
    g["expression_bindings"]["expression"]["unwrap_groups"] = serde_json::json!(false);
    l.graph = serde_json::from_value(g).unwrap();
    let disabled = analyze(l, source, "group-off");
    assert_eq!(calls(&disabled)[0].resolution, ResolutionState::Unresolved);
}
#[test]
fn assignments_and_declared_unary_mutations_invalidate_only_the_affected_binding() {
    for (name, source) in [
        ("after", "FUNC f() {} EVAL f(); EVAL f = 1;"),
        ("before", "FUNC f() {} EVAL f = 1; EVAL f();"),
        ("unary", "FUNC f() {} EVAL f(); EVAL f++;"),
    ] {
        let r = analyze(language("brace_mold"), source, name);
        assert_eq!(calls(&r)[0].resolution, ResolutionState::Unresolved);
        assert_eq!(
            serde_json::to_value(calls(&r)[0]).unwrap()["resolution_reason"],
            "binding_is_mutated"
        );
    }
    let r = analyze(
        language("brace_mold"),
        "FUNC f() {} FUNC outer(f) { EVAL f = 1; } EVAL f();",
        "other-binding",
    );
    assert_eq!(calls(&r)[0].resolution, ResolutionState::Resolved);
}
#[test]
fn declaration_visibility_parent_search_and_kind_filters_remain_declarative() {
    let source = "FUNC outer() { EVAL f(); FUNC f() {} EVAL f(); }";
    let mut l = language("brace_mold");
    l.grammar
        .productions
        .iter_mut()
        .find(|p| p.name == "function")
        .unwrap()
        .projection
        .scope
        .as_mut()
        .unwrap()
        .visibility = nexora_core::DeclarationVisibility::AfterDeclaration;
    let r = analyze(l, source, "visibility");
    let cs = calls(&r);
    assert_eq!(cs[0].resolution, ResolutionState::Unresolved);
    assert_eq!(cs[1].resolution, ResolutionState::Resolved);
    let mut l = language("brace_mold");
    l.grammar
        .productions
        .iter_mut()
        .find(|p| p.name == "function")
        .unwrap()
        .projection
        .scope
        .as_mut()
        .unwrap()
        .parent_lookup = false;
    let r = analyze(l, "FUNC f() {} FUNC outer() { EVAL f(); }", "no-parent");
    assert_eq!(calls(&r)[0].resolution, ResolutionState::Unresolved);
    let mut l = language("brace_mold");
    l.graph
        .expression_bindings
        .get_mut("expression")
        .unwrap()
        .target_kinds = vec!["Function".into()];
    let r = analyze(
        l,
        "FUNC f() {} FUNC outer(f) { EVAL f(); }",
        "filter-shadow",
    );
    assert_eq!(calls(&r)[0].resolution, ResolutionState::Unresolved);
}
#[test]
fn incomplete_regions_never_confirm_uses_or_calls_and_sites_have_exact_evidence() {
    let source = "FUNC café(p) {\r\n EVAL p;\r\n} EVAL café(1); EVAL 2 +;";
    let r = analyze(language("brace_mold"), source, "partial");
    assert!(
        r.graph
            .edges
            .iter()
            .filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::References))
            .all(|e| e.resolution == ResolutionState::Unresolved)
    );
    let good = analyze(
        language("brace_mold"),
        "FUNC café(p) {\r\n EVAL p;\r\n} EVAL café(1); EVAL café(2);",
        "evidence",
    );
    let cs = calls(&good);
    assert_eq!(cs.len(), 2);
    assert_ne!(cs[0].id, cs[1].id);
    assert_eq!(cs[0].target, cs[1].target);
    for e in good
        .graph
        .edges
        .iter()
        .filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::References))
    {
        let n = good.graph.nodes.iter().find(|n| n.id == e.source).unwrap();
        assert_eq!(n.evidence.as_ref().unwrap().span, e.evidence.span);
        let mut start = nexora_core::SourcePosition::default();
        start.advance(&good.source.content[..e.evidence.span.start.offset]);
        assert_eq!(start, e.evidence.span.start);
    }
}
#[test]
fn expression_binding_strategies_require_versioned_valid_data() {
    for mode in [
        "version",
        "missing-context",
        "missing-kind",
        "call-kind",
        "duplicate",
        "mutation",
    ] {
        let mut l = language("brace_mold");
        match mode {
            "version" => l.graph.schema_version = 4,
            "missing-context" => {
                let r = l.graph.expression_bindings.remove("expression").unwrap();
                l.graph.expression_bindings.insert("unknown".into(), r);
            }
            "missing-kind" => l
                .graph
                .expression_bindings
                .get_mut("expression")
                .unwrap()
                .target_kinds
                .push("Class".into()),
            "call-kind" => l
                .graph
                .expression_bindings
                .get_mut("expression")
                .unwrap()
                .callable_kinds
                .push("Class".into()),
            "duplicate" => l
                .graph
                .expression_bindings
                .get_mut("expression")
                .unwrap()
                .target_kinds
                .push("Function".into()),
            _ => l
                .graph
                .expression_bindings
                .get_mut("expression")
                .unwrap()
                .mutating_unary_operators
                .push("unknown".into()),
        };
        assert!(l.validate().is_err(), "{mode}");
    }
}
#[test]
fn exhausted_shared_lookup_budget_cannot_confirm_an_earlier_direct_site() {
    let mut l = language("brace_mold");
    l.grammar.max_steps = 2_000_000;
    let source = format!(
        "FUNC target() {{}} EVAL target(); {} {}",
        "FUNC f() {} ".repeat(200),
        "EVAL f(); ".repeat(501)
    );
    let r = analyze(l, &source, "budget");
    assert!(r.graph.edges.iter().any(
        |e| serde_json::to_value(e).unwrap()["resolution_reason"] == "binding_budget_exhausted"
    ));
    assert!(
        calls(&r)
            .iter()
            .all(|e| e.resolution != ResolutionState::Resolved)
    );
    assert!(r.diagnostics.len() <= 1024);
}
#[test]
fn incremental_bindings_match_full_analysis_after_shadowing_and_mutation_edits() {
    use nexora_core::TenantId;
    use nexora_indexer::{IndexCache, MemoryStore};
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-bind-cache-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.nxb"), "FUNC f() {} EVAL f();").unwrap();
    fs::write(root.join("b.nxb"), "FUNC f() {} EVAL f();").unwrap();
    let indexer = Indexer::new(language("brace_mold"));
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
    fs::write(root.join("a.nxb"), "FUNC f() {} EVAL f(); EVAL f = 1;").unwrap();
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
    let cs: Vec<_> = full
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .collect();
    assert_eq!(
        cs.iter()
            .filter(|e| e.resolution == ResolutionState::Resolved)
            .count(),
        1
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn grouped_mutation_targets_invalidate_bindings_even_when_callee_unwrapping_is_disabled() {
    for (index, suffix) in ["(f)++", "((f))++"].iter().enumerate() {
        let mut l = language("brace_mold");
        l.graph
            .expression_bindings
            .get_mut("expression")
            .unwrap()
            .unwrap_groups = false;
        let r = analyze(
            l,
            &format!("FUNC f() {{}} EVAL f(); EVAL {suffix};"),
            &format!("grouped-write-{index}"),
        );
        assert_eq!(calls(&r)[0].resolution, ResolutionState::Unresolved);
        assert!(
            r.graph
                .edges
                .iter()
                .any(|e| e.kind == EdgeKind::References && e.label.as_deref() == Some("write"))
        );
    }
}
#[test]
fn call_confirmation_requires_binding_coverage_for_every_expression_context() {
    let mut l = language("brace_mold");
    let mut alternate = l.grammar.contexts["expression"].clone();
    alternate.expression.as_mut().unwrap().ast_kind = "alternate_expression".into();
    l.grammar.contexts.insert("unbound".into(), alternate);
    l.graph
        .mappings
        .insert("alternate_expression".into(), "Expression".into());
    l.grammar
        .productions
        .iter_mut()
        .find(|p| p.name == "expression_statement")
        .unwrap()
        .projection
        .regions[0]
        .context = Some("unbound".into());
    assert!(l.validate().is_err());
}
