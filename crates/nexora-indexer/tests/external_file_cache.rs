use nexora_core::TenantId;
use nexora_indexer::{CacheFragment, ExternalFileCache, FileCacheKey, Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::{cell::RefCell, collections::BTreeMap, fs, path::Path};

#[test]
fn external_fragments_survive_new_indexers_and_match_full_snapshots() {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-external-cache-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.js"), "function hello() {}\n").unwrap();
    fs::write(dir.join("b.js"), "class World {}\n").unwrap();
    let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    let records = RefCell::new(BTreeMap::<FileCacheKey, CacheFragment>::new());
    let mut read = |key: &FileCacheKey| Ok(records.borrow().get(key).cloned());
    let mut write = |key: &FileCacheKey, value: &CacheFragment| {
        records.borrow_mut().insert(key.clone(), value.clone());
        Ok(true)
    };
    let mut external = ExternalFileCache {
        read: &mut read,
        write: &mut write,
    };
    let cold = Indexer::new(language.clone())
        .index_external(
            &dir,
            TenantId("public".into()),
            &mut MemoryStore::default(),
            &mut external,
        )
        .unwrap();
    assert_eq!(cold.reuse.analyzed_files, 2);
    let warm = Indexer::new(language.clone())
        .index_external(
            &dir,
            TenantId("public".into()),
            &mut MemoryStore::default(),
            &mut external,
        )
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 2);
    assert_eq!(warm.reuse.analyzed_files, 0);
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    fs::write(dir.join("b.js"), "class Changed {}\n").unwrap();
    let changed = Indexer::new(language.clone())
        .index_external(
            &dir,
            TenantId("public".into()),
            &mut MemoryStore::default(),
            &mut external,
        )
        .unwrap();
    assert_eq!(changed.reuse.reused_files, 1);
    assert_eq!(changed.reuse.analyzed_files, 1);
    let full = Indexer::new(language.clone())
        .index(&dir, TenantId("public".into()), &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(full).unwrap(),
        serde_json::to_value(&changed.snapshot).unwrap()
    );
    let isolated = Indexer::new(language)
        .index_external(
            &dir,
            TenantId("different".into()),
            &mut MemoryStore::default(),
            &mut external,
        )
        .unwrap();
    assert_eq!(isolated.reuse.reused_files, 0);
    assert_eq!(isolated.reuse.analyzed_files, 2);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn corrupt_external_fragment_cannot_erase_a_file_from_the_snapshot() {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-external-corrupt-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.js"), "class Kept {}\n").unwrap();
    let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    let mut read = |key: &FileCacheKey| {
        Ok(Some(CacheFragment {
            schema_version: 2,
            payload_hash: "invalid".into(),
            key: key.clone(),
            graph: Default::default(),
            diagnostics: vec![],
            computation_us: 1,
        }))
    };
    let mut write = |_: &FileCacheKey, _: &CacheFragment| Ok(true);
    let mut cache = ExternalFileCache {
        read: &mut read,
        write: &mut write,
    };
    let result = Indexer::new(language.clone())
        .index_external(
            &dir,
            TenantId("public".into()),
            &mut MemoryStore::default(),
            &mut cache,
        )
        .unwrap();
    assert_eq!(result.reuse.analyzed_files, 1);
    let full = Indexer::new(language)
        .index(&dir, TenantId("public".into()), &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(full).unwrap(),
        serde_json::to_value(result.snapshot).unwrap()
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn external_cache_preserves_embedded_units_bindings_and_call_evidence() {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-external-composed-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let root = Path::new("../../templates/composition-calls");
    let source = fs::read_to_string(root.join("fixtures/complete/source.nxb")).unwrap();
    fs::write(dir.join("source.nxb"), source).unwrap();
    let host = LanguageDefinition::load(&root.join("region_carrier")).unwrap();
    let provider = LanguageDefinition::load(&root.join("region_expression")).unwrap();
    let make = || Indexer::with_catalog(host.clone(), vec![provider.clone()]).unwrap();
    let entries = RefCell::new(BTreeMap::<FileCacheKey, CacheFragment>::new());
    let mut read = |key: &FileCacheKey| Ok(entries.borrow().get(key).cloned());
    let mut write = |key: &FileCacheKey, value: &CacheFragment| {
        entries.borrow_mut().insert(key.clone(), value.clone());
        Ok(true)
    };
    let mut cache = ExternalFileCache {
        read: &mut read,
        write: &mut write,
    };
    let cold = make()
        .index_external(
            &dir,
            TenantId("public".into()),
            &mut MemoryStore::default(),
            &mut cache,
        )
        .unwrap();
    let warm = make()
        .index_external(
            &dir,
            TenantId("public".into()),
            &mut MemoryStore::default(),
            &mut cache,
        )
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 1);
    let full = make()
        .index(&dir, TenantId("public".into()), &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    assert_eq!(
        serde_json::to_value(full).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    assert!(
        warm.snapshot
            .graph
            .edges
            .iter()
            .any(|e| e.kind == nexora_graph::EdgeKind::Calls)
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn valid_json_corruption_cannot_change_facts_for_unchanged_source() {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-external-checksum-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.js"), "class Original {}\n").unwrap();
    let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    let entries = RefCell::new(BTreeMap::<FileCacheKey, CacheFragment>::new());
    let mut read = |key: &FileCacheKey| Ok(entries.borrow().get(key).cloned());
    let mut write = |key: &FileCacheKey, value: &CacheFragment| {
        entries.borrow_mut().insert(key.clone(), value.clone());
        Ok(true)
    };
    let cold = {
        let mut cache = ExternalFileCache {
            read: &mut read,
            write: &mut write,
        };
        Indexer::new(language.clone())
            .index_external(
                &dir,
                TenantId("public".into()),
                &mut MemoryStore::default(),
                &mut cache,
            )
            .unwrap()
    };
    let key = entries.borrow().keys().next().unwrap().clone();
    let mut value = serde_json::to_value(entries.borrow().get(&key).unwrap()).unwrap();
    let class = value["graph"]["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|node| node["kind"] == "Class")
        .unwrap();
    class["name"] = serde_json::json!("AlteredWithoutChangingSource");
    entries
        .borrow_mut()
        .insert(key, serde_json::from_value(value).unwrap());
    let mut cache = ExternalFileCache {
        read: &mut read,
        write: &mut write,
    };
    let repaired = Indexer::new(language)
        .index_external(
            &dir,
            TenantId("public".into()),
            &mut MemoryStore::default(),
            &mut cache,
        )
        .unwrap();
    assert_eq!(repaired.reuse.analyzed_files, 1);
    assert_eq!(
        serde_json::to_value(cold.snapshot).unwrap(),
        serde_json::to_value(repaired.snapshot).unwrap()
    );
    fs::remove_dir_all(dir).unwrap();
}
