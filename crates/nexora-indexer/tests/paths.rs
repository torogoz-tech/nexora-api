use nexora_core::TenantId;
use nexora_indexer::{Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::{fs, path::Path};
#[test]
fn source_larger_than_five_mib_is_analyzed_without_a_file_size_ceiling() {
    let path = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-large-{}.js", std::process::id()));
    fs::write(&path, vec![b' '; 5 * 1024 * 1024 + 1]).unwrap();
    let indexer =
        Indexer::new(LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap());
    let result = indexer.analyze(&path);
    assert!(result.is_ok(), "{result:?}");
    fs::remove_file(path).unwrap();
}
#[cfg(unix)]
#[test]
fn symlink_file_and_ancestor_are_rejected() {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-links-{}", std::process::id()));
    fs::create_dir_all(dir.join("real")).unwrap();
    fs::write(dir.join("real/a.js"), "class A {}").unwrap();
    std::os::unix::fs::symlink(dir.join("real/a.js"), dir.join("link.js")).unwrap();
    std::os::unix::fs::symlink(dir.join("real"), dir.join("alias")).unwrap();
    let indexer =
        Indexer::new(LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap());
    assert!(indexer.analyze(&dir.join("link.js")).is_err());
    assert!(indexer.analyze(&dir.join("alias/a.js")).is_err());
    assert!(
        indexer
            .index(&dir, TenantId("a".into()), &mut MemoryStore::default())
            .is_err()
    );
    fs::remove_dir_all(dir).unwrap();
}
