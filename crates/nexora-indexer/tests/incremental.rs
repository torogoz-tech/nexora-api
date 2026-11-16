use nexora_core::TenantId;
use nexora_indexer::{IndexCache, Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::{
    fs,
    path::{Path, PathBuf},
};
fn root(name: &str) -> PathBuf {
    let p = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-incremental-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}
fn pack() -> LanguageDefinition {
    LanguageDefinition::load(Path::new("../../templates/language-pack")).unwrap()
}
fn same(indexer: &Indexer, path: &Path, snapshot: &nexora_storage::Snapshot) {
    let full = indexer
        .index(
            path,
            snapshot.tenant_id.clone(),
            &mut MemoryStore::default(),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::to_value(snapshot).unwrap()
    );
}
#[test]
fn unchanged_files_skip_analysis_and_same_size_same_mtime_edit_is_detected() {
    let root = root("content");
    fs::write(root.join("a.nxexample"), "DEF alpha [] BEGIN FINISH\n").unwrap();
    fs::write(root.join("b.nxexample"), "DEF bravo [] BEGIN FINISH\n").unwrap();
    let indexer = Indexer::new(pack());
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let cold = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(cold.reuse.analyzed_files, 2);
    assert_eq!(cold.reuse.reused_files, 0);
    assert!(!cold.reuse.plan_reused);
    same(&indexer, &root, &cold.snapshot);
    let warm = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(warm.reuse.analyzed_files, 0);
    assert_eq!(warm.reuse.reused_files, 2);
    assert!(warm.reuse.plan_reused);
    assert_eq!(cold.snapshot.id, warm.snapshot.id);
    assert_eq!(warm.timings.lexer_ms, 0.0);
    assert_eq!(warm.timings.parser_ms, 0.0);
    same(&indexer, &root, &warm.snapshot);
    let path = root.join("a.nxexample");
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, "DEF alter [] BEGIN FINISH\n").unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let changed = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(changed.reuse.analyzed_files, 1);
    assert_eq!(changed.reuse.reused_files, 1);
    assert_ne!(changed.snapshot.id, cold.snapshot.id);
    assert!(cold.snapshot.graph.nodes.iter().any(|n| n.name == "alpha"));
    same(&indexer, &root, &changed.snapshot);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn added_removed_and_ambiguous_targets_re_resolve_unchanged_importers() {
    let root = root("targets");
    fs::write(root.join("a.lua"), "require('storage')\n").unwrap();
    let indexer = Indexer::new(LanguageDefinition::load(Path::new("../../languages/lua")).unwrap());
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let missing = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(missing.snapshot.diagnostics.len(), 1);
    same(&indexer, &root, &missing.snapshot);
    fs::write(root.join("storage.lua"), "function saved() end\n").unwrap();
    let resolved = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(resolved.reuse.reused_files, 1);
    assert!(resolved.snapshot.diagnostics.is_empty());
    assert!(
        resolved
            .snapshot
            .graph
            .edges
            .iter()
            .filter(|e| e.kind == nexora_graph::EdgeKind::Imports)
            .all(|e| e.resolution == nexora_graph::ResolutionState::Resolved)
    );
    same(&indexer, &root, &resolved.snapshot);
    fs::create_dir(root.join("storage")).unwrap();
    fs::write(root.join("storage/init.lua"), "function alternate() end\n").unwrap();
    let ambiguous = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(ambiguous.reuse.reused_files, 2);
    assert!(
        ambiguous
            .snapshot
            .graph
            .edges
            .iter()
            .filter(|e| e.kind == nexora_graph::EdgeKind::Imports)
            .all(|e| e.resolution == nexora_graph::ResolutionState::Ambiguous)
    );
    same(&indexer, &root, &ambiguous.snapshot);
    fs::remove_file(root.join("storage.lua")).unwrap();
    fs::remove_dir_all(root.join("storage")).unwrap();
    let removed = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(removed.reuse.reused_files, 1);
    assert_eq!(removed.reuse.removed_files, 2);
    assert_eq!(removed.snapshot.diagnostics.len(), 1);
    same(&indexer, &root, &removed.snapshot);
    fs::remove_dir_all(root).unwrap();
}
#[path = "support/rejecting_store.rs"]
mod rejecting_store;
#[test]
fn partial_and_failed_sources_are_retried_and_renames_rebind_evidence() {
    let root = root("errors");
    fs::write(root.join("a.nxexample"), "DEF good [] BEGIN FINISH\n").unwrap();
    fs::write(root.join("bad.nxexample"), "\"bad").unwrap();
    fs::write(root.join("partial.nxexample"), "DEF incomplete [\n").unwrap();
    let indexer = Indexer::new(pack());
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let cold = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(cold.reuse.retained_files, 1);
    assert_eq!(cold.snapshot.diagnostics.len(), 2);
    let warm = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(warm.reuse.analyzed_files, 2);
    assert_eq!(warm.reuse.reused_files, 1);
    assert_eq!(warm.snapshot.id, cold.snapshot.id);
    same(&indexer, &root, &warm.snapshot);
    fs::write(root.join("bad.nxexample"), "DEF fixed [] BEGIN FINISH\n").unwrap();
    fs::rename(root.join("a.nxexample"), root.join("renamed.nxexample")).unwrap();
    let fixed = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(fixed.reuse.removed_files, 1);
    assert_eq!(fixed.reuse.retained_files, 2);
    assert!(!fixed.snapshot.sources.contains_key("a.nxexample"));
    assert!(
        fixed
            .snapshot
            .graph
            .nodes
            .iter()
            .filter_map(|n| n.evidence.as_ref())
            .all(|e| e.path != "a.nxexample")
    );
    same(&indexer, &root, &fixed.snapshot);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn scopes_and_same_version_configuration_changes_invalidate_reuse() {
    let root = root("scope");
    fs::write(root.join("a.nxexample"), "DEF alpha [] BEGIN FINISH\n").unwrap();
    let indexer = Indexer::new(pack());
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    let tenant = indexer
        .index_incremental(&root, TenantId("b".into()), &mut store, &mut cache)
        .unwrap();
    assert!(tenant.reuse.cache_reset);
    assert_eq!(tenant.reuse.reused_files, 0);
    same(&indexer, &root, &tenant.snapshot);
    let mut changed = pack();
    changed
        .graph
        .mappings
        .insert("function".into(), "Method".into());
    let changed = Indexer::new(changed);
    let update = changed
        .index_incremental(&root, TenantId("b".into()), &mut store, &mut cache)
        .unwrap();
    assert!(update.reuse.cache_reset);
    assert_eq!(update.reuse.reused_files, 0);
    assert!(
        update
            .snapshot
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == nexora_graph::NodeKind::Method)
    );
    assert_ne!(update.snapshot.id, tenant.snapshot.id);
    same(&changed, &root, &update.snapshot);
    let other = root.join("other");
    fs::create_dir(&other).unwrap();
    fs::copy(root.join("a.nxexample"), other.join("a.nxexample")).unwrap();
    let moved = changed
        .index_incremental(&other, TenantId("b".into()), &mut store, &mut cache)
        .unwrap();
    assert!(moved.reuse.cache_reset);
    assert_eq!(moved.reuse.reused_files, 0);
    same(&changed, &other, &moved.snapshot);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn capacity_zero_and_small_cache_never_change_snapshot_results() {
    let root = root("capacity");
    fs::write(root.join("a.nxexample"), "DEF alpha [] BEGIN FINISH\n").unwrap();
    fs::write(root.join("b.nxexample"), "DEF bravo [] BEGIN FINISH\n").unwrap();
    let indexer = Indexer::new(pack());
    let mut store = MemoryStore::default();
    let mut unlimited = IndexCache::default();
    let measured = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut unlimited)
        .unwrap();
    let one_file = measured.reuse.cache_weight_bytes / 2 + 32;
    for capacity in [0, 1, one_file] {
        let mut cache = IndexCache::with_capacity(capacity).unwrap();
        for _ in 0..2 {
            let pass = indexer
                .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
                .unwrap();
            assert!(pass.reuse.cache_weight_bytes <= capacity);
            assert!(pass.reuse.retained_files < 2);
            assert_eq!(pass.snapshot.id, measured.snapshot.id);
            same(&indexer, &root, &pass.snapshot);
        }
    }
    assert!(IndexCache::with_capacity(67_108_865).is_err());
    unlimited.clear();
    assert_eq!(unlimited.weight_bytes(), 0);
    assert_eq!(unlimited.retained_files(), 0);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn publication_failure_preserves_previous_cache_generation() {
    let root = root("atomic");
    fs::write(root.join("a.nxexample"), "DEF alpha [] BEGIN FINISH\n").unwrap();
    let indexer = Indexer::new(pack());
    let mut cache = IndexCache::default();
    let mut store = rejecting_store::RejectingStore::default();
    let first = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    let weight = cache.weight_bytes();
    fs::write(root.join("a.nxexample"), "DEF bravo [] BEGIN FINISH\n").unwrap();
    store.reject = true;
    assert!(
        indexer
            .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
            .is_err()
    );
    assert_eq!(cache.weight_bytes(), weight);
    fs::write(root.join("a.nxexample"), "DEF alpha [] BEGIN FINISH\n").unwrap();
    let recovered = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(recovered.snapshot.id, first.snapshot.id);
    assert_eq!(recovered.reuse.reused_files, 1);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn resolution_budget_is_fresh_on_every_warm_pass() {
    let root = root("budget");
    fs::write(root.join("a.lua"), "require('storage')\n").unwrap();
    fs::write(root.join("storage.lua"), "function saved() end\n").unwrap();
    let mut language = LanguageDefinition::load(Path::new("../../languages/lua")).unwrap();
    language.manifest.resolution.as_mut().unwrap().max_steps = 2;
    let indexer = Indexer::new(language);
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    for pass in 0..4 {
        let result = indexer
            .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
            .unwrap();
        assert!(result.snapshot.diagnostics.is_empty());
        assert_eq!(result.reuse.reused_files, if pass == 0 { 0 } else { 2 });
        same(&indexer, &root, &result.snapshot);
    }
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn all_registered_reference_packs_have_identical_full_and_warm_snapshots() {
    for (pack, input) in [
        (
            "../../languages/javascript",
            "../../corpus/fixtures/javascript/allowed",
        ),
        ("../../languages/lua", "../../corpus/fixtures/lua/allowed"),
        (
            "../../templates/language-pack",
            "../../corpus/fixtures/template/allowed",
        ),
    ] {
        let indexer = Indexer::new(LanguageDefinition::load(Path::new(pack)).unwrap());
        let mut cache = IndexCache::default();
        let mut store = MemoryStore::default();
        let first = indexer
            .index_incremental(
                Path::new(input),
                TenantId("test".into()),
                &mut store,
                &mut cache,
            )
            .unwrap();
        let second = indexer
            .index_incremental(
                Path::new(input),
                TenantId("test".into()),
                &mut store,
                &mut cache,
            )
            .unwrap();
        assert_eq!(second.reuse.reused_files, 2);
        assert_eq!(first.snapshot.id, second.snapshot.id);
        same(&indexer, Path::new(input), &second.snapshot);
    }
}
#[cfg(unix)]
#[test]
fn hot_cache_does_not_bypass_symlink_validation() {
    let root = root("symlink");
    fs::write(root.join("a.nxexample"), "DEF alpha [] BEGIN FINISH\n").unwrap();
    let indexer = Indexer::new(pack());
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    std::os::unix::fs::symlink(root.join("a.nxexample"), root.join("alias.nxexample")).unwrap();
    assert!(
        indexer
            .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
            .is_err()
    );
    assert_eq!(cache.retained_files(), 1);
    fs::remove_file(root.join("alias.nxexample")).unwrap();
    let recovered = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(recovered.reuse.reused_files, 1);
    fs::remove_dir_all(root).unwrap();
}
