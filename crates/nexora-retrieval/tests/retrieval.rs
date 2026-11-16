use nexora_core::TenantId;
use nexora_indexer::{Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use nexora_retrieval::{FullTextSearch, GraphTraversal, RetrievalEngine, SymbolSearch};
use std::path::Path;
#[test]
fn queries_use_real_source_and_graph_with_limits() {
    let indexer =
        Indexer::new(LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap());
    let snapshot = indexer
        .index(
            Path::new("../../tests/fixtures/javascript"),
            TenantId("local".into()),
            &mut MemoryStore::default(),
        )
        .unwrap();
    let engine = RetrievalEngine::new(&snapshot);
    assert_eq!(engine.find_symbol("greet", 10).len(), 1);
    assert!(engine.find_symbol("missing", 10).is_empty());
    assert!(engine.find_symbol("greet", 0).is_empty());
    let matches = engine.search_code("return", 1);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].path, "declarations.js");
    assert_eq!(matches[0].line, 3);
    let file = snapshot
        .graph
        .nodes
        .iter()
        .find(|n| n.name == "declarations.js")
        .unwrap();
    assert_eq!(engine.traverse(&file.id, 1, 100).len(), 3);
    assert_eq!(engine.traverse(&file.id, 0, 100).len(), 1);
    assert!(engine.search_code("", 10).is_empty());
}
