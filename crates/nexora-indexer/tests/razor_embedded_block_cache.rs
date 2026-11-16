use nexora_core::TenantId;
use nexora_indexer::{IndexCache, Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::path::Path;

#[test]
fn block_views_reuse_cache_and_refresh_after_source_and_provider_changes() {
    let directory = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "nexora-embedded-block-cache-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("public.razor");
    std::fs::write(&path, "@{ var x=1; <p>@Name</p> } @(After())").unwrap();
    let root = Path::new("../../templates/dotnet-understanding");
    let host = LanguageDefinition::load(&root.join("razor")).unwrap();
    let mut child = LanguageDefinition::load(&root.join("csharp")).unwrap();
    let indexer = Indexer::with_catalog(host.clone(), vec![child.clone()]).unwrap();
    let mut store = MemoryStore::default();
    let mut cache = IndexCache::default();
    let tenant = TenantId("public-embedded-block".into());
    let cold = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    let warm = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert!(cold.snapshot.diagnostics.is_empty());
    assert_eq!(warm.reuse.reused_files, 1);
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    let source = "@{ var x=1; <section>@Name</section> } @(After())";
    std::fs::write(&path, source).unwrap();
    let edited = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(edited.reuse.analyzed_files, 1);
    let name: Vec<_> = edited
        .snapshot
        .graph
        .syntax_units
        .values()
        .filter(|u| &source[u.span.start.offset..u.span.end.offset] == "Name")
        .collect();
    assert_eq!(name.len(), 1);
    let full = indexer
        .index(&directory, tenant.clone(), &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&edited.snapshot).unwrap(),
        serde_json::to_value(full).unwrap()
    );
    child
        .lexer
        .entry_contexts
        .get_mut("razor_body")
        .unwrap()
        .retain(|kind| kind != "body_element" && kind != "unknown_body_less");
    let changed = Indexer::with_catalog(host, vec![child]).unwrap();
    let refreshed = changed
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(refreshed.reuse.analyzed_files, 1);
    assert!(
        refreshed
            .snapshot
            .diagnostics
            .iter()
            .any(|d| d.stage == "lexer")
    );
    assert_ne!(refreshed.snapshot.id, edited.snapshot.id);
    let full = changed
        .index(&directory, tenant, &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&refreshed.snapshot).unwrap(),
        serde_json::to_value(full).unwrap()
    );
    std::fs::remove_dir_all(directory).unwrap();
}
