use nexora_core::{RepositoryId, TenantId};
use nexora_indexer::{ExternalFileCache, Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::{
    collections::BTreeMap,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn logical_repository_identity_survives_checkout_moves_and_separates_file_cache_scopes() {
    let parent = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "nexora-logical-repository-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let original = parent.join("original");
    let moved = parent.join("moved");
    std::fs::create_dir_all(&original).unwrap();
    std::fs::write(original.join("source.js"), "function greet() {}\n").unwrap();
    let indexer =
        Indexer::new(LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap());
    let tenant = TenantId("installation-test".into());
    let repository = RepositoryId("repo_registered_1".into());
    let first = indexer
        .index_repository(
            &original,
            tenant.clone(),
            repository.clone(),
            &mut MemoryStore::default(),
        )
        .unwrap();
    assert_eq!(first.repository_id, repository);
    let legacy = indexer
        .index(&original, tenant.clone(), &mut MemoryStore::default())
        .unwrap();
    assert_ne!(legacy.repository_id, repository);
    std::fs::rename(&original, &moved).unwrap();
    let second = indexer
        .index_repository(
            &moved,
            tenant.clone(),
            repository.clone(),
            &mut MemoryStore::default(),
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&second).unwrap()
    );
    let mut fragments = BTreeMap::new();
    let mut read = |_key: &nexora_indexer::FileCacheKey| Ok(None);
    let mut write = |key: &nexora_indexer::FileCacheKey,
                     fragment: &nexora_indexer::CacheFragment| {
        fragments.insert(key.clone(), fragment.clone());
        Ok(true)
    };
    let cold = indexer
        .index_repository_external(
            &moved,
            tenant.clone(),
            repository.clone(),
            &mut MemoryStore::default(),
            &mut ExternalFileCache {
                read: &mut read,
                write: &mut write,
            },
        )
        .unwrap();
    assert_eq!(cold.reuse.analyzed_files, 1);
    let mut read = |key: &nexora_indexer::FileCacheKey| Ok(fragments.get(key).cloned());
    let mut write = |_: &nexora_indexer::FileCacheKey, _: &nexora_indexer::CacheFragment| Ok(true);
    let warm = indexer
        .index_repository_external(
            &moved,
            tenant.clone(),
            repository,
            &mut MemoryStore::default(),
            &mut ExternalFileCache {
                read: &mut read,
                write: &mut write,
            },
        )
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 1);
    let other = indexer
        .index_repository_external(
            &moved,
            tenant.clone(),
            RepositoryId("repo_registered_2".into()),
            &mut MemoryStore::default(),
            &mut ExternalFileCache {
                read: &mut read,
                write: &mut write,
            },
        )
        .unwrap();
    assert_eq!(other.reuse.reused_files, 0);
    assert_eq!(other.reuse.analyzed_files, 1);
    for invalid in [
        "",
        "repo_",
        "repo_bad/path",
        "repo_white space",
        legacy.repository_id.0.as_str(),
    ] {
        assert!(
            indexer
                .index_repository(
                    &moved,
                    tenant.clone(),
                    RepositoryId(invalid.into()),
                    &mut MemoryStore::default()
                )
                .is_err()
        );
    }
    std::fs::remove_dir_all(parent).unwrap();
}
