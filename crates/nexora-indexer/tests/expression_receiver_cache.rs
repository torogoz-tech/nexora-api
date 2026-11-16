use nexora_core::TenantId;
use nexora_indexer::{IndexCache, Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use serde_json::json;
use std::{fs, path::Path};

#[test]
fn same_version_receiver_policy_edit_invalidates_cache_and_matches_full_analysis() {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-receiver-cache-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("public.brace"),
        "VAR a; VAR value=(a)(1); VAR after=2;",
    )
    .unwrap();
    let mut language =
        LanguageDefinition::load(Path::new("../../templates/semantic-molds/brace_mold")).unwrap();
    // Use the model's extension rather than assuming a particular suffix.
    let file = dir.join(format!("public.{}", language.manifest.extensions[0]));
    fs::rename(dir.join("public.brace"), file).unwrap();
    language.grammar.schema_version = 10;
    language.validate().unwrap();
    let tenant = TenantId("public-receiver-cache".into());
    let mut store = MemoryStore::default();
    let mut cache = IndexCache::default();
    let original = Indexer::new(language.clone());
    let cold = original
        .index_incremental(&dir, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert!(cold.snapshot.diagnostics.is_empty());
    let warm = original
        .index_incremental(&dir, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 1);
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    language
        .grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap()
        .application[0]
        .excluded_receivers = serde_json::from_value(json!([{"kinds":["group"]}])).unwrap();
    language.validate().unwrap();
    let changed = Indexer::new(language);
    let refreshed = changed
        .index_incremental(&dir, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(refreshed.reuse.analyzed_files, 1);
    assert_ne!(warm.snapshot.id, refreshed.snapshot.id);
    assert!(!refreshed.snapshot.diagnostics.is_empty());
    let full = changed
        .index(&dir, tenant, &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(full).unwrap(),
        serde_json::to_value(refreshed.snapshot).unwrap()
    );
    fs::remove_dir_all(dir).unwrap();
}
