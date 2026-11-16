use nexora_graph::{EdgeKind, NodeKind, ResolutionState};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use std::path::Path;
#[test]
fn two_molds_preserve_nested_ownership_and_shadowing() {
    for (id, extension) in [("brace_mold", "nxb"), ("keyword_mold", "nxk")] {
        let directory = Path::new("../../templates/semantic-molds").join(id);
        let language = LanguageDefinition::load(&directory).unwrap();
        let result = Indexer::new(language)
            .analyze(&directory.join(format!("nested.{extension}")))
            .unwrap();
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.symbols.scopes.len(), 3);
        assert_eq!(
            result
                .symbols
                .symbols
                .iter()
                .filter(|s| s.name == "value")
                .count(),
            2
        );
        assert!(result.ast.nodes.iter().any(|n|n.name=="p"&&serde_json::to_value(n).unwrap()["parent_id"].is_string()));
        let values: Vec<_> = result
            .symbols
            .symbols
            .iter()
            .filter(|s| s.name == "value")
            .collect();
        assert_ne!(values[0].scope_id, values[1].scope_id);
        let refs: Vec<_> = result
            .graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::References)
            .collect();
        assert_eq!(refs.len(), 3);
        assert!(
            refs.iter()
                .all(|e| e.resolution == ResolutionState::Resolved)
        );
        assert_ne!(refs[0].target, refs[2].target);
        let variable_nodes: Vec<_> = result
            .graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Variable)
            .collect();
        assert_eq!(variable_nodes.len(), 2);
        assert!(
            result
                .graph
                .edges
                .iter()
                .filter(|e| e.kind == EdgeKind::Declares
                    && variable_nodes.iter().any(|n| n.id == e.target))
                .all(|e| result
                    .graph
                    .nodes
                    .iter()
                    .find(|n| n.id == e.source)
                    .unwrap()
                    .kind
                    != NodeKind::File)
        );
    }
}
fn language() -> LanguageDefinition {
    LanguageDefinition::load(Path::new("../../templates/semantic-molds/brace_mold")).unwrap()
}
fn analyze(
    language: LanguageDefinition,
    source: &str,
    name: &str,
) -> nexora_indexer::AnalysisResult {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-body-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("source.nxb");
    std::fs::write(&path, source).unwrap();
    let result = Indexer::new(language).analyze(&path).unwrap();
    std::fs::remove_dir_all(root).unwrap();
    result
}
#[test]
fn visibility_parent_lookup_and_shadowing_are_template_policies() {
    let source = "FUNC outer() { READ value; VAR value; READ value; } VAR value;";
    let sequential = analyze(language(), source, "sequential");
    assert!(sequential.diagnostics.is_empty());
    let refs: Vec<_> = sequential
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::References)
        .collect();
    assert_ne!(refs[0].target, refs[1].target);
    let mut hoisted = language();
    hoisted.grammar.productions[0]
        .projection
        .scope
        .as_mut()
        .unwrap()
        .visibility = nexora_core::DeclarationVisibility::WholeScope;
    let hoisted = analyze(hoisted, source, "hoisted");
    assert!(hoisted.diagnostics.is_empty());
    let refs: Vec<_> = hoisted
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::References)
        .collect();
    assert_eq!(refs[0].target, refs[1].target);
    let mut isolated = language();
    isolated.grammar.productions[0]
        .projection
        .scope
        .as_mut()
        .unwrap()
        .parent_lookup = false;
    let isolated = analyze(isolated, source, "isolated");
    let refs: Vec<_> = isolated
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::References)
        .collect();
    assert_eq!(refs[0].resolution, ResolutionState::Unresolved);
    assert_eq!(refs[1].resolution, ResolutionState::Resolved);
    let mut collected = language();
    collected.grammar.productions[0]
        .projection
        .scope
        .as_mut()
        .unwrap()
        .shadowing = nexora_core::ShadowingPolicy::Collect;
    let collected = analyze(collected, source, "collect");
    let refs: Vec<_> = collected
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::References)
        .collect();
    assert_eq!(refs[0].resolution, ResolutionState::Resolved);
    assert_eq!(refs[1].resolution, ResolutionState::Ambiguous);
}
#[test]
fn empty_unicode_crlf_regions_and_opaque_bodies_have_honest_coverage() {
    let result = analyze(language(), "FUNC café() { \r\n }", "empty");
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.ast.nodes.len(), 1);
    assert_eq!(result.ast.nodes[0].regions.len(), 2);
    assert!(
        result.ast.nodes[0]
            .regions
            .iter()
            .all(|r| r.status == nexora_core::RegionStatus::Parsed)
    );
    assert_eq!(result.symbols.scopes.len(), 2);
    for region in &result.ast.nodes[0].regions {
        let mut start = nexora_core::SourcePosition::default();
        start.advance(&result.source.content[..region.span.start.offset]);
        assert_eq!(start, region.span.start);
        let mut end = start;
        end.advance(&result.source.content[region.span.start.offset..region.span.end.offset]);
        assert_eq!(end, region.span.end);
    }
    let mut opaque = language();
    opaque.grammar.productions[0].projection.regions[1].context = None;
    let opaque = analyze(opaque, "FUNC outer(p) { VAR hidden; }", "opaque");
    assert!(opaque.diagnostics.is_empty());
    assert_eq!(opaque.symbols.symbols.len(), 2);
    assert!(!opaque.symbols.complete);
    assert_eq!(opaque.graph.syntax_coverage["source.nxb"].opaque_regions, 1);
    assert!(!opaque.graph.nodes.iter().any(|n| n.name == "hidden"));
    let flat =
        Indexer::new(LanguageDefinition::load(Path::new("../../templates/language-pack")).unwrap())
            .analyze(Path::new(
                "../../tests/fixtures/template-repository/domain/order.nxexample",
            ))
            .unwrap();
    assert!(
        !flat
            .graph
            .syntax_coverage
            .values()
            .next()
            .unwrap()
            .hierarchical
    );
}
#[test]
fn region_errors_do_not_reparent_siblings_and_shared_budget_leaves_pending_regions_partial() {
    let bad = analyze(
        language(),
        "FUNC outer() { BAD; VAR inside; } VAR outside;",
        "recovery",
    );
    assert!(!bad.diagnostics.is_empty());
    assert!(
        bad.ast
            .nodes
            .iter()
            .find(|n| n.name == "inside")
            .unwrap()
            .parent_id
            .is_some()
    );
    assert!(
        bad.ast
            .nodes
            .iter()
            .find(|n| n.name == "outside")
            .unwrap()
            .parent_id
            .is_none()
    );
    assert_eq!(
        bad.ast.nodes[0].regions[1].status,
        nexora_core::RegionStatus::Partial
    );
    let mut limited = language();
    limited.grammar.max_steps = 30;
    let limited = analyze(
        limited,
        "FUNC a() { VAR x; } FUNC b() { VAR y; }",
        "shared-budget",
    );
    assert!(
        limited
            .diagnostics
            .iter()
            .any(|d| d.message.contains("budget"))
    );
    assert_eq!(
        limited
            .ast
            .nodes
            .iter()
            .filter(|n| n.kind == "function")
            .count(),
        2
    );
    assert!(limited.graph.syntax_coverage["source.nxb"].partial_regions > 0);
    assert!(!limited.symbols.complete);
}
#[test]
fn same_start_parent_child_ids_are_distinct_and_nonshrinking_regions_fail() {
    let mut l = language();
    let p: nexora_language::ProductionRule=serde_json::from_value(serde_json::json!({"name":"wrapper","pattern":{"type":"sequence","items":[{"type":"capture","name":"inside","item":{"type":"sequence","items":[{"type":"literal","text":"VAR"},{"type":"capture","name":"name","item":{"type":"token","kind":"identifier"},"join":""},{"type":"literal","text":";"}]}},{"type":"literal","text":";"}]},"projection":{"ast_kind":"block","role":"Container","name_capture":"name","scope":{"visibility":"after_declaration","parent_lookup":true,"shadowing":"nearest"},"regions":[{"capture":"inside","context":"inside","trim_start":0,"trim_end":0}]}})).unwrap();
    l.grammar.productions.push(p);
    l.grammar.root_productions = vec!["wrapper".into()];
    l.grammar.contexts.insert(
        "inside".into(),
        serde_json::from_value(serde_json::json!({"productions":["variable"],"ignored":[]}))
            .unwrap(),
    );
    let result = analyze(l.clone(), "VAR x;;", "same-start");
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.ast.nodes.len(), 2);
    assert_eq!(
        result.ast.nodes[0].span.start,
        result.ast.nodes[1].span.start
    );
    assert_ne!(result.ast.nodes[0].id, result.ast.nodes[1].id);
    assert_eq!(
        result.ast.nodes[1].parent_id.as_ref(),
        Some(&result.ast.nodes[0].id)
    );
    l.grammar.productions.last_mut().unwrap().pattern=serde_json::from_value(serde_json::json!({"type":"capture","name":"inside","item":{"type":"sequence","items":[{"type":"literal","text":"VAR"},{"type":"capture","name":"name","item":{"type":"token","kind":"identifier"},"join":""},{"type":"literal","text":";"}]}})).unwrap();
    let invalid = analyze(l, "VAR x;", "no-shrink");
    assert!(
        invalid
            .diagnostics
            .iter()
            .any(|d| d.message.contains("strictly smaller"))
    );
    assert!(invalid.ast.nodes.is_empty());
}
#[test]
fn invalid_contexts_regions_and_legacy_schema_changes_are_rejected() {
    for mode in [
        "root",
        "context",
        "capture",
        "legacy",
        "trim",
        "hidden-group",
        "overlap-config",
    ] {
        let mut l = language();
        match mode {
            "root" => l.grammar.root_productions = vec!["missing".into()],
            "context" => {
                l.grammar.productions[0].projection.regions[0].context = Some("missing".into())
            }
            "capture" => l.grammar.productions[0].projection.regions[0].capture = "missing".into(),
            "legacy" => l.grammar.schema_version = 2,
            "trim" => l.grammar.productions[0].projection.regions[0].trim_start = 257,
            "hidden-group" => l.grammar.productions[0].projection.regions.clear(),
            _ => {
                let duplicate = l.grammar.productions[0].projection.regions[0].clone();
                l.grammar.productions[0].projection.regions.push(duplicate);
            }
        }
        assert!(l.validate().is_err(), "{mode}");
    }
}
#[test]
fn scoped_graph_and_region_metadata_survive_incremental_reuse() {
    use nexora_core::TenantId;
    use nexora_indexer::{IndexCache, MemoryStore};
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-scope-snapshots-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source =
        "FUNC outer(p) { VAR value; BLOCK { VAR value; READ value; READ p; } READ value; }";
    std::fs::write(root.join("a.nxb"), source).unwrap();
    std::fs::write(root.join("b.nxb"), "FUNC other() { VAR v; }").unwrap();
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
    assert_eq!(warm.snapshot.graph.syntax_coverage.len(), 2);
    assert!(
        warm.snapshot
            .graph
            .nodes
            .iter()
            .any(|n| n.owned_scope_id.is_some() && !n.regions.is_empty())
    );
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    std::fs::write(
        root.join("a.nxb"),
        source.replace("BLOCK { VAR value;", "BLOCK { VAR renamed;"),
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
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn region_rule_expansion_is_bounded_before_other_validation() {
    let mut l = language();
    l.grammar.rules.insert(
        "r0".into(),
        serde_json::from_value(serde_json::json!({"type":"literal","text":";"})).unwrap(),
    );
    for i in 1..=35 {
        l.grammar.rules.insert(format!("r{i}"),serde_json::from_value(serde_json::json!({"type":"sequence","items":[{"type":"rule","name":format!("r{}",i-1)},{"type":"rule","name":format!("r{}",i-1)}]})).unwrap());
    }
    l.grammar.ignored =
        vec![serde_json::from_value(serde_json::json!({"type":"rule","name":"r35"})).unwrap()];
    assert!(l.validate().is_err());
}
#[test]
fn adjacent_regions_do_not_share_end_exclusive_errors() {
    let mut l = language();
    let p:nexora_language::ProductionRule=serde_json::from_value(serde_json::json!({"name":"adjacent","pattern":{"type":"sequence","items":[{"type":"capture","name":"left","item":{"type":"sequence","items":[{"type":"literal","text":"VAR"},{"type":"capture","name":"name","item":{"type":"token","kind":"identifier"},"join":""},{"type":"literal","text":";"}]}},{"type":"capture","name":"right","item":{"type":"sequence","items":[{"type":"literal","text":"BAD"},{"type":"literal","text":";"}]}},{"type":"literal","text":";"}]},"projection":{"ast_kind":"block","role":"Container","name_capture":"name","regions":[{"capture":"left","context":"body"},{"capture":"right","context":"body"}]}})).unwrap();
    l.grammar.productions.push(p);
    l.grammar.root_productions = vec!["adjacent".into()];
    let result = analyze(l, "VAR x;BAD;;", "adjacent");
    assert!(!result.diagnostics.is_empty());
    let regions = &result.ast.nodes[0].regions;
    assert_eq!(regions[0].span.end, regions[1].span.start);
    assert_eq!(regions[0].status, nexora_core::RegionStatus::Parsed);
    assert_eq!(regions[1].status, nexora_core::RegionStatus::Partial);
}
#[test]
fn nested_functions_own_their_parameters_and_do_not_leak_to_file_scope() {
    let result = analyze(
        language(),
        "FUNC outer(p) { FUNC inner(p) { READ p; } READ p; } READ inner;",
        "nested-functions",
    );
    assert_eq!(result.symbols.scopes.len(), 3);
    let parameters: Vec<_> = result
        .symbols
        .symbols
        .iter()
        .filter(|s| s.name == "p")
        .collect();
    assert_eq!(parameters.len(), 2);
    assert_ne!(parameters[0].scope_id, parameters[1].scope_id);
    let inner = result
        .symbols
        .symbols
        .iter()
        .find(|s| s.name == "inner")
        .unwrap();
    assert_eq!(inner.scope_id, parameters[0].scope_id);
    let refs: Vec<_> = result
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::References)
        .collect();
    assert_eq!(refs.len(), 3);
    assert_eq!(refs[0].resolution, ResolutionState::Resolved);
    assert_eq!(refs[1].resolution, ResolutionState::Resolved);
    assert_ne!(refs[0].target, refs[1].target);
    assert_eq!(refs[2].resolution, ResolutionState::Unresolved);
}
#[test]
fn full_owner_opaque_capture_is_coverage_only_and_never_recurses() {
    let mut l = language();
    let p:nexora_language::ProductionRule=serde_json::from_value(serde_json::json!({"name":"opaque_fragment","pattern":{"type":"capture","name":"whole","item":{"type":"literal","text":"OPAQUE"}},"projection":{"ast_kind":"statement","role":"Container","name_capture":"whole","regions":[{"capture":"whole"}]}})).unwrap();
    l.grammar.productions.push(p);
    l.grammar.root_productions = vec!["opaque_fragment".into()];
    let r = analyze(l, "OPAQUE", "whole-opaque");
    assert!(r.diagnostics.is_empty());
    assert_eq!(r.ast.nodes[0].regions[0].span, r.ast.nodes[0].span);
    assert_eq!(
        r.ast.nodes[0].regions[0].status,
        nexora_core::RegionStatus::Opaque
    );
    assert!(!r.symbols.complete);
}
