mod counting_store;
use crate::benchmark_comparison::BenchmarkComparison;
use counting_store::CountingStore;
use nexora_core::TenantId;
use nexora_indexer::{IndexCache, Indexer};
use nexora_language::LanguageDefinition;
#[test]
fn reference_and_incremental_publish_identical_histories() {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-benchmark-parity-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("source.nxexample"), "DEF good [] BEGIN FINISH\n").unwrap();
    let indexer = Indexer::new(
        LanguageDefinition::load(std::path::Path::new("../../templates/language-pack")).unwrap(),
    );
    let mut cache = IndexCache::default();
    let mut incremental = CountingStore::default();
    let mut reference = CountingStore::default();
    for _ in 0..2 {
        let (result, full) = BenchmarkComparison::measure(
            &indexer,
            &root,
            TenantId("test".into()),
            &mut cache,
            &mut incremental,
            &mut reference,
        )
        .unwrap();
        assert_eq!(result.snapshot.id, full.snapshot.id);
        assert_eq!(incremental.publications, 1);
        assert_eq!(reference.publications, 1);
    }
    std::fs::write(root.join("source.nxexample"), "DEF other [] BEGIN FINISH\n").unwrap();
    BenchmarkComparison::measure(
        &indexer,
        &root,
        TenantId("test".into()),
        &mut cache,
        &mut incremental,
        &mut reference,
    )
    .unwrap();
    assert_eq!(incremental.publications, 2);
    assert_eq!(reference.publications, 2);
    std::fs::remove_dir_all(root).unwrap();
}
