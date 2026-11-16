use nexora_core::TenantId;
use nexora_indexer::{IndexCache, Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::path::Path;
#[test]
fn unconsumed_parent_text_and_same_version_rule_edits_invalidate_cached_evidence() {
    let directory = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-boundary-cache-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("source.razor");
    std::fs::write(&path, "@Name; tail").unwrap();
    let profiles = Path::new("../../templates/dotnet-understanding");
    let host = LanguageDefinition::load(&profiles.join("razor")).unwrap();
    let child = LanguageDefinition::load(&profiles.join("csharp")).unwrap();
    let indexer = Indexer::with_catalog(host.clone(), vec![child.clone()]).unwrap();
    let tenant = TenantId("boundary-cache".into());
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let cold = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    let warm = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 1);
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    std::fs::write(&path, "@Name! tail").unwrap();
    let changed = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(changed.reuse.analyzed_files, 1);
    let before = changed
        .snapshot
        .graph
        .nodes
        .iter()
        .filter_map(|n| n.lexical_region.as_ref())
        .find(|r| r.mode == "implicit_expression")
        .unwrap();
    assert_eq!(before.content_span.end.offset, 5);
    assert_eq!(before.lookahead_span.unwrap().end.offset, 7);
    let full = indexer
        .index(&directory, tenant.clone(), &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::to_value(&changed.snapshot).unwrap()
    );
    let mut edited = host;
    let rule = &mut edited
        .lexer
        .regions
        .get_mut("implicit_expression")
        .unwrap()
        .close_before[0];
    rule.pattern = format!("!|(?:{})", rule.pattern);
    let edited = Indexer::with_catalog(edited, vec![child]).unwrap();
    let refreshed = edited
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(refreshed.reuse.analyzed_files, 1);
    assert_ne!(refreshed.snapshot.id, changed.snapshot.id);
    let after = refreshed
        .snapshot
        .graph
        .nodes
        .iter()
        .filter_map(|n| n.lexical_region.as_ref())
        .find(|r| r.mode == "implicit_expression")
        .unwrap();
    assert_eq!(after.content_span.end.offset, 5);
    assert_eq!(after.lookahead_span.unwrap().end.offset, 6);
    let full = edited
        .index(&directory, tenant, &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::to_value(&refreshed.snapshot).unwrap()
    );
    std::fs::remove_dir_all(directory).unwrap();
}
