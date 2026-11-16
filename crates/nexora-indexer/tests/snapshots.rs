use nexora_core::TenantId;
use nexora_indexer::{Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::{fs, path::Path};
#[test]
fn snapshots_are_deterministic_and_change_with_content() {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-snapshots-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.js"), "function same() {}").unwrap();
    fs::write(dir.join("b.js"), "class Same {}").unwrap();
    let indexer =
        Indexer::new(LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap());
    let mut store = MemoryStore::default();
    let first = indexer
        .index(&dir, TenantId("a".into()), &mut store)
        .unwrap();
    assert_eq!(first.sources.len(), 2);
    assert_eq!(
        first
            .graph
            .nodes
            .iter()
            .map(|n| &n.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        5
    );
    assert_eq!(
        first.id,
        indexer
            .index(&dir, TenantId("a".into()), &mut store)
            .unwrap()
            .id
    );
    fs::write(dir.join("b.js"), "class Changed {}").unwrap();
    let changed = indexer
        .index(&dir, TenantId("a".into()), &mut store)
        .unwrap();
    assert_ne!(first.id, changed.id);
    assert!(first.graph.nodes.iter().any(|n| n.name == "Same"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn invalid_file_does_not_hide_valid_file_or_diagnostic_origin() {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-file-errors-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("broken.js"), "'unterminated").unwrap();
    fs::write(dir.join("valid.js"), "class Good {}").unwrap();
    let indexer =
        Indexer::new(LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap());
    let snapshot = indexer
        .index(&dir, TenantId("a".into()), &mut MemoryStore::default())
        .unwrap();
    assert_eq!(snapshot.sources.len(), 2);
    assert!(snapshot.graph.nodes.iter().any(|n| n.name == "Good"));
    assert_eq!(snapshot.diagnostics.len(), 1);
    assert_eq!(snapshot.diagnostics[0].path.as_deref(), Some("broken.js"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn changing_invalid_source_changes_snapshot_identity_and_preserves_context() {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-invalid-hash-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("broken.js"), "'unterminated").unwrap();
    let indexer =
        Indexer::new(LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap());
    let mut store = MemoryStore::default();
    let first = indexer
        .index(&dir, TenantId("a".into()), &mut store)
        .unwrap();
    fs::write(dir.join("broken.js"), "'different").unwrap();
    let second = indexer
        .index(&dir, TenantId("a".into()), &mut store)
        .unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(first.sources["broken.js"], "'unterminated");
    assert_eq!(second.sources["broken.js"], "'different");
    fs::remove_dir_all(dir).unwrap();
}
