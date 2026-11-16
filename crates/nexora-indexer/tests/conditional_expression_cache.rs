use nexora_core::TenantId;
use nexora_indexer::{IndexCache, Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::{fs, path::Path};

#[test]
fn conditional_branch_policy_invalidates_cache_without_a_version_change() {
    let directory = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-conditional-cache-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("public.ts");
    fs::write(
        &path,
        "function pick(flag, a, b) { return flag ? a=1 : b=2; }",
    )
    .unwrap();
    let profile =
        LanguageDefinition::load(Path::new("../../templates/web-understanding/typescript"))
            .unwrap();
    let indexer = Indexer::with_catalog(profile.clone(), vec![]).unwrap();
    let tenant = TenantId("public-conditional".into());
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
    let mut changed = profile;
    changed
        .grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap()
        .conditional[0]
        .consequent_precedence = 21;
    let changed = Indexer::with_catalog(changed, vec![]).unwrap();
    let refreshed = changed
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(refreshed.reuse.analyzed_files, 1);
    assert_ne!(cold.snapshot.id, refreshed.snapshot.id);
    assert!(
        refreshed
            .snapshot
            .diagnostics
            .iter()
            .any(|d| d.stage == "parser")
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
