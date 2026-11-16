use nexora_core::{RegionStatus, TenantId};
use nexora_indexer::{IndexCache, Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::path::{Path, PathBuf};

fn profiles() -> (LanguageDefinition, LanguageDefinition) {
    let root = Path::new("../../templates/composition-molds");
    let mut host = LanguageDefinition::load(&root.join("region_carrier")).unwrap();
    host.manifest
        .composition
        .as_mut()
        .unwrap()
        .routes
        .get_mut("code")
        .unwrap()
        .context = "body".into();
    (
        host,
        LanguageDefinition::load(&root.join("region_expression")).unwrap(),
    )
}
fn directory(name: &str) -> PathBuf {
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "nexora-composed-indexing-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}
fn same(indexer: &Indexer, root: &Path, snapshot: &nexora_storage::Snapshot) {
    let full = indexer
        .index(
            root,
            snapshot.tenant_id.clone(),
            &mut MemoryStore::default(),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(full).unwrap(),
        serde_json::to_value(snapshot).unwrap()
    );
}

#[test]
fn ordinary_analysis_returns_host_and_interior_syntax_with_a_unified_graph() {
    let root = directory("analysis");
    let path = root.join("source.nxb");
    std::fs::write(&path, "VAR host; ~[@(VAR local; READ local;)]~").unwrap();
    let (host, target) = profiles();
    let result = Indexer::with_catalog(host, vec![target])
        .unwrap()
        .analyze_measured(&path)
        .unwrap();
    assert_eq!(result.result.embedded_units.len(), 1);
    assert_eq!(result.result.interpretations.len(), 1);
    assert_eq!(result.result.graph.syntax_units.len(), 2);
    assert_eq!(result.result.symbols.symbols.len(), 1);
    assert_eq!(
        result.result.embedded_units[0].symbols.symbols[0].name,
        "local"
    );
    assert!(result.timings.composition_ms > 0.0);
    assert!(result.result.diagnostics.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cold_warm_and_interior_edit_snapshots_preserve_full_composition_metadata() {
    let root = directory("cache");
    for name in ["a.nxb", "b.nxb"] {
        std::fs::write(root.join(name), "VAR host; ~[@(VAR local; READ local;)]~").unwrap();
    }
    let (host, target) = profiles();
    let indexer = Indexer::with_catalog(host, vec![target]).unwrap();
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let cold = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(cold.reuse.analyzed_files, 2);
    assert_eq!(cold.snapshot.graph.syntax_units.len(), 4);
    same(&indexer, &root, &cold.snapshot);
    let warm = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 2);
    assert_eq!(warm.reuse.analyzed_files, 0);
    assert_eq!(warm.timings.composition_ms, 0.0);
    same(&indexer, &root, &warm.snapshot);
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    std::fs::write(
        root.join("a.nxb"),
        "VAR host; ~[@(VAR other; READ other;)]~",
    )
    .unwrap();
    let edited = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(edited.reuse.analyzed_files, 1);
    assert_eq!(edited.reuse.reused_files, 1);
    assert_ne!(edited.snapshot.id, cold.snapshot.id);
    same(&indexer, &root, &edited.snapshot);
    assert!(!cold.snapshot.graph.nodes.iter().any(|n| n.name == "other"));
    assert!(
        edited
            .snapshot
            .graph
            .nodes
            .iter()
            .any(|n| n.name == "other")
    );
    assert_eq!(edited.snapshot.graph.unit_coverage.len(), 4);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn dependency_changes_at_same_version_reset_cache_and_change_snapshot_identity() {
    let root = directory("dependency");
    std::fs::write(root.join("a.nxb"), "~[@(VAR local; READ local;)]~").unwrap();
    let (host, target) = profiles();
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let a = Indexer::with_catalog(host.clone(), vec![target.clone()])
        .unwrap()
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    let mut target = target;
    target.grammar.max_steps -= 1;
    let b = Indexer::with_catalog(host, vec![target])
        .unwrap()
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert!(b.reuse.cache_reset);
    assert_eq!(b.reuse.reused_files, 0);
    assert_ne!(a.snapshot.id, b.snapshot.id);
    assert_ne!(
        a.snapshot.analysis_configuration,
        b.snapshot.analysis_configuration
    );
    assert!(b.snapshot.analysis_engine.is_some());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn interior_errors_are_preserved_and_not_cached_as_clean_analysis() {
    let root = directory("errors");
    std::fs::write(root.join("a.nxb"), "~[@(VAR local; READ local;)]~").unwrap();
    std::fs::write(root.join("bad.nxb"), "~[@(x @ y)]~ VAR after;").unwrap();
    let (host, target) = profiles();
    let indexer = Indexer::with_catalog(host, vec![target]).unwrap();
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let first = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert!(first.snapshot.graph.nodes.iter().any(|n| n.name == "after"));
    assert!(
        first
            .snapshot
            .graph
            .syntax_units
            .values()
            .any(|u| u.status == RegionStatus::Partial)
    );
    assert!(
        first
            .snapshot
            .diagnostics
            .iter()
            .any(|d| d.stage == "lexer" && d.path.as_deref() == Some("bad.nxb"))
    );
    let warm = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 1);
    assert_eq!(warm.reuse.analyzed_files, 1);
    same(&indexer, &root, &warm.snapshot);
    std::fs::remove_dir_all(root).unwrap();
}

fn module_profiles() -> (LanguageDefinition, LanguageDefinition) {
    let (mut host, mut target) = profiles();
    let js = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    for (language, suffix) in [(&mut host, ".nxb"), (&mut target, ".special.nxb")] {
        let mut resolution = js.manifest.resolution.clone().unwrap();
        for route in &mut resolution.routes {
            route.candidates.truncate(1);
            route.candidates[0].template = format!("{{module}}{suffix}");
        }
        language.manifest.resolution = Some(resolution);
        language.grammar.productions.push(serde_json::from_value(serde_json::json!({
            "name":"import", "pattern":{"type":"sequence","items":[{"type":"literal","text":"USE"},{"type":"capture","name":"name","item":{"type":"token","kind":"string"},"join":""},{"type":"literal","text":";"}]},
            "projection":{"ast_kind":"module_ref","role":"Reference","name_capture":"name"}
        })).unwrap());
        language.grammar.root_productions.push("import".into());
        language
            .grammar
            .contexts
            .get_mut("body")
            .unwrap()
            .productions
            .push("import".into());
        language
            .graph
            .mappings
            .insert("module_ref".into(), "Module".into());
        language.graph.reference_relations.insert(
            "module_ref".into(),
            vec!["IMPORTS".into(), "DEPENDS_ON".into()],
        );
        language.validate().unwrap();
    }
    (host, target)
}

#[test]
fn modules_use_each_units_strategy_and_cached_importers_are_resolved_again() {
    use nexora_graph::{EdgeKind, ResolutionState};
    let root = directory("modules");
    std::fs::write(
        root.join("main.nxb"),
        "USE \"./root\"; ~[@(USE \"./inner\";)]~",
    )
    .unwrap();
    std::fs::write(root.join("root.nxb"), "VAR root;").unwrap();
    std::fs::write(root.join("inner.special.nxb"), "VAR inner;").unwrap();
    std::fs::write(root.join("inner.nxb"), "VAR wrong;").unwrap();
    let (host, target) = module_profiles();
    let indexer = Indexer::with_catalog(host, vec![target]).unwrap();
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let first = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    let imports: Vec<_> = first
        .snapshot
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Imports)
        .collect();
    assert_eq!(imports.len(), 2);
    assert!(
        imports
            .iter()
            .all(|e| e.resolution == ResolutionState::Resolved)
    );
    let interior_import = imports
        .iter()
        .find(|e| {
            e.evidence
                .syntax_unit
                .as_ref()
                .and_then(|id| first.snapshot.graph.syntax_units.get(id))
                .is_some_and(|u| u.language.0 == "region_expression")
        })
        .unwrap();
    assert_eq!(
        first
            .snapshot
            .graph
            .nodes
            .iter()
            .find(|n| n.id == interior_import.target)
            .unwrap()
            .name,
        "inner.special.nxb"
    );
    assert!(first.snapshot.diagnostics.is_empty());
    same(&indexer, &root, &first.snapshot);
    assert!(
        first
            .snapshot
            .source_coverage
            .iter()
            .filter(|c| c.path == "main.nxb")
            .all(|c| !c.complete)
    );
    std::fs::remove_file(root.join("inner.special.nxb")).unwrap();
    let missing = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(missing.reuse.reused_files, 3);
    assert_eq!(missing.reuse.removed_files, 1);
    assert!(
        missing
            .snapshot
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Imports && e.resolution == ResolutionState::Unresolved)
    );
    assert!(
        missing
            .snapshot
            .diagnostics
            .iter()
            .any(|d| d.stage == "module-resolution")
    );
    same(&indexer, &root, &missing.snapshot);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn registry_collects_only_reachable_dependencies_and_handles_cycles() {
    let (host, target) = profiles();
    let mut registry = nexora_language::LanguageRegistry::default();
    registry.register(host.clone()).unwrap();
    registry.register(target.clone()).unwrap();
    let mut unrelated = target;
    unrelated.manifest.id.0 = "unused".into();
    unrelated
        .manifest
        .composition
        .as_mut()
        .unwrap()
        .routes
        .get_mut("code")
        .unwrap()
        .language
        .0 = "missing".into();
    registry.register(unrelated).unwrap();
    let dependencies = registry.composition_dependencies(&host).unwrap();
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0].manifest.id.0, "region_expression");
    let mut missing = nexora_language::LanguageRegistry::default();
    missing.register(host.clone()).unwrap();
    assert!(missing.composition_dependencies(&host).is_err());
}

#[path = "support/rejecting_store.rs"]
mod rejecting_store;
#[test]
fn failed_publication_keeps_old_cache_generation_and_tenant_changes_reanalyze() {
    let root = directory("publication");
    let path = root.join("a.nxb");
    std::fs::write(&path, "~[@(VAR local; READ local;)]~").unwrap();
    let (host, target) = profiles();
    let indexer = Indexer::with_catalog(host, vec![target]).unwrap();
    let mut cache = IndexCache::default();
    let mut store = rejecting_store::RejectingStore::default();
    let old = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    let weight = cache.weight_bytes();
    std::fs::write(&path, "~[@(VAR other; READ other;)]~").unwrap();
    store.reject = true;
    assert!(
        indexer
            .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
            .is_err()
    );
    assert_eq!(cache.weight_bytes(), weight);
    store.reject = false;
    let retry = indexer
        .index_incremental(&root, TenantId("a".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(retry.reuse.analyzed_files, 1);
    assert_eq!(retry.reuse.reused_files, 0);
    assert_ne!(retry.snapshot.id, old.snapshot.id);
    same(&indexer, &root, &retry.snapshot);
    let tenant = indexer
        .index_incremental(&root, TenantId("b".into()), &mut store, &mut cache)
        .unwrap();
    assert!(tenant.reuse.cache_reset);
    assert_eq!(tenant.reuse.reused_files, 0);
    assert_eq!(tenant.snapshot.tenant_id.0, "b");
    let mut zero = IndexCache::with_capacity(0).unwrap();
    let no_cache = indexer
        .index_incremental(&root, TenantId("b".into()), &mut store, &mut zero)
        .unwrap();
    assert_eq!(no_cache.reuse.retained_files, 0);
    same(&indexer, &root, &no_cache.snapshot);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_interior_module_reference_can_resolve_to_its_own_source_file() {
    use nexora_graph::{EdgeKind, NodeKind, ResolutionState};
    let root = directory("self-module");
    std::fs::write(root.join("main.special.nxb"), "~[@(USE \"./main\";)]~").unwrap();
    let (host, target) = module_profiles();
    let indexer = Indexer::with_catalog(host, vec![target]).unwrap();
    let snapshot = indexer
        .index(&root, TenantId("a".into()), &mut MemoryStore::default())
        .unwrap();
    let edge = snapshot
        .graph
        .edges
        .iter()
        .find(|e| e.kind == EdgeKind::Imports)
        .unwrap();
    assert_eq!(edge.resolution, ResolutionState::Resolved);
    let target = snapshot
        .graph
        .nodes
        .iter()
        .find(|n| n.id == edge.target)
        .unwrap();
    assert_eq!(target.kind, NodeKind::File);
    assert_eq!(target.name, "main.special.nxb");
    assert_ne!(edge.evidence.syntax_unit, target.syntax_unit);
    assert!(snapshot.diagnostics.is_empty());
    same(&indexer, &root, &snapshot);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn composed_source_larger_than_one_mib_preserves_embedded_analysis() {
    let root = directory("large-composition");
    let path = root.join("source.nxb");
    let mut source = " ".repeat(1_048_577);
    source.push_str("VAR host; ~[@(VAR local; READ local;)]~");
    std::fs::write(&path, source).unwrap();
    let (host, target) = profiles();
    let result = Indexer::with_catalog(host, vec![target])
        .unwrap()
        .analyze(&path)
        .unwrap();
    assert_eq!(result.embedded_units.len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
