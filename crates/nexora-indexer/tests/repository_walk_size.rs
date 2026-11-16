use nexora_indexer::RepositoryWalker;

#[test]
fn traversal_does_not_abort_after_ten_thousand_files() {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-walk-size-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for index in 0..10_001 {
        std::fs::write(root.join(format!("file-{index}.js")), "").unwrap();
    }
    let result = RepositoryWalker::collect(&root);
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(result.unwrap().len(), 10_001);
}
