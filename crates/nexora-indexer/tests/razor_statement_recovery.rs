use nexora_indexer::{CompositionAnalyzer, CompositionResult};
use nexora_language::LanguageDefinition;
use std::path::Path;

fn analyze(source: &str) -> CompositionResult {
    let root = Path::new("../../templates/dotnet-understanding");
    CompositionAnalyzer::new(
        LanguageDefinition::load(&root.join("razor")).unwrap(),
        vec![LanguageDefinition::load(&root.join("csharp")).unwrap()],
    )
    .unwrap()
    .analyze_source("public.razor", source)
    .unwrap()
}

#[test]
fn missing_terminator_keeps_only_the_statement_partial_and_preserves_followers() {
    for source in [
        "@if (Ready) { Unfinished } @(After())",
        r#"@if (Ready) { var text="}"; Unfinished } @(After())"#,
        r#"@if (Ready) { var item=new Entry { Text="}" }; Unfinished } @(After())"#,
    ] {
        let result = analyze(source);
        let regions: Vec<_> = result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .collect();
        let statement = regions
            .iter()
            .find(|r| r.mode == "csharp_statement" && !r.closed)
            .unwrap();
        let parent = regions
            .iter()
            .find(|r| r.id == statement.parent.clone().unwrap())
            .unwrap();
        assert!(parent.closed);
        assert_eq!(statement.span.end, parent.closer.unwrap().start);
        assert!(statement.closer.is_none());
        assert_eq!(
            &source[statement.span.start.offset..statement.span.end.offset],
            "Unfinished "
        );
        assert_eq!(regions.iter().filter(|r| !r.closed).count(), 1);
        assert_eq!(
            result
                .units
                .iter()
                .filter(|u| &source[u.span.start.offset..u.span.end.offset] == "After()")
                .count(),
            1
        );
        let graph: nexora_graph::CodeGraph =
            serde_json::from_value(serde_json::to_value(&result.graph).unwrap()).unwrap();
        nexora_graph::GraphIntegrityValidator::validate(&graph).unwrap();
        assert!(!result.complete);
        assert_eq!(result.units[0].status, nexora_core::RegionStatus::Partial);
        assert!(
            result
                .graph
                .edges
                .iter()
                .all(|e| e.kind != nexora_graph::EdgeKind::Calls)
        );
    }
}

#[test]
fn eof_and_literal_closing_braces_do_not_fabricate_a_parent_closure() {
    let source = r#"@if (Ready) { var text="}"; Unfinished"#;
    let result = analyze(source);
    let regions: Vec<_> = result.units[0]
        .tokens
        .iter()
        .flat_map(|t| &t.regions)
        .collect();
    assert!(regions.iter().any(|r| r.mode == "conditional" && !r.closed));
    assert!(
        regions
            .iter()
            .any(|r| r.mode == "csharp_statement" && !r.closed)
    );
    assert!(!result.complete);
}

#[test]
fn partial_recovery_retries_and_matches_full_after_same_version_rule_change() {
    use nexora_core::TenantId;
    use nexora_indexer::{IndexCache, Indexer, MemoryStore};
    let directory = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-statement-recovery-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = "@if (Ready) { Unfinished } @(After())";
    std::fs::write(directory.join("public.razor"), source).unwrap();
    let root = Path::new("../../templates/dotnet-understanding");
    let mut host = LanguageDefinition::load(&root.join("razor")).unwrap();
    let child = LanguageDefinition::load(&root.join("csharp")).unwrap();
    let indexer = Indexer::with_catalog(host.clone(), vec![child.clone()]).unwrap();
    let tenant = TenantId("public-statement-recovery".into());
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let cold = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    let retry = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(retry.reuse.reused_files, 0);
    assert_eq!(retry.reuse.cache_skipped_files, 1);
    assert!(
        retry
            .snapshot
            .graph
            .syntax_units
            .values()
            .any(|u| &source[u.span.start.offset..u.span.end.offset] == "After()")
    );
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&retry.snapshot).unwrap()
    );
    let full = indexer
        .index(&directory, tenant.clone(), &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::to_value(&retry.snapshot).unwrap()
    );
    host.lexer
        .regions
        .get_mut("csharp_statement")
        .unwrap()
        .recovery_prefixes
        .clear();
    let changed = Indexer::with_catalog(host, vec![child]).unwrap();
    let refreshed = changed
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(refreshed.reuse.analyzed_files, 1);
    assert_ne!(refreshed.snapshot.id, retry.snapshot.id);
    assert!(
        !refreshed
            .snapshot
            .graph
            .syntax_units
            .values()
            .any(|u| &source[u.span.start.offset..u.span.end.offset] == "After()")
    );
    let full = changed
        .index(&directory, tenant, &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::to_value(&refreshed.snapshot).unwrap()
    );
    std::fs::remove_dir_all(directory).unwrap();
}
