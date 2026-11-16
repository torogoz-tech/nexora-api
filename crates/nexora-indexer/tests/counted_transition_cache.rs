use nexora_core::TenantId;
use nexora_indexer::{IndexCache, Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::{fs, path::Path};

#[test]
fn counted_capture_and_same_version_placement_edits_match_full_analysis() {
    let directory = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-counted-cache-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("public.cs");
    fs::write(
        &path,
        r####"var text=$$"""{{{value}}}"""; class After {}"####,
    )
    .unwrap();
    let profile =
        LanguageDefinition::load(Path::new("../../templates/dotnet-understanding/csharp")).unwrap();
    let indexer = Indexer::with_catalog(profile.clone(), vec![]).unwrap();
    let tenant = TenantId("public-counted".into());
    let mut store = MemoryStore::default();
    let mut cache = IndexCache::default();
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
    fs::write(
        &path,
        r####"var text=$$$"""{{{{value}}}}"""; class After {}"####,
    )
    .unwrap();
    let edited = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(edited.reuse.analyzed_files, 1);
    let full = indexer
        .index(&directory, tenant.clone(), &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(full).unwrap(),
        serde_json::to_value(&edited.snapshot).unwrap()
    );
    let mut changed = profile;
    changed
        .lexer
        .regions
        .get_mut("interpolated_raw")
        .unwrap()
        .counted_children[0]
        .open_at_end = false;
    let changed = Indexer::with_catalog(changed, vec![]).unwrap();
    let refreshed = changed
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(refreshed.reuse.analyzed_files, 1);
    assert_ne!(edited.snapshot.id, refreshed.snapshot.id);
    assert!(
        refreshed
            .snapshot
            .diagnostics
            .iter()
            .all(|d| d.stage != "indexer")
    );
    let full = changed
        .index(&directory, tenant, &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(full).unwrap(),
        serde_json::to_value(refreshed.snapshot).unwrap()
    );
    fs::remove_dir_all(directory).unwrap();
}
