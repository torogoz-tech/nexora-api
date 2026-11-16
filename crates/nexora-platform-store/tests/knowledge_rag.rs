#[test]
fn rag_migration_declares_rebuildable_processing_and_embedding_state() {
    let migration = include_str!("../migrations/0110_knowledge_rag.sql");
    for required in [
        "knowledge_processing_jobs",
        "knowledge_structures",
        "knowledge_chunks",
        "knowledge_embeddings",
        "content_hash",
        "embedding_model",
        "project_id",
        "source_revision",
    ] {
        assert!(migration.contains(required), "missing {required}");
    }
    let code_migration = include_str!("../migrations/0112_code_snapshot_embeddings.sql");
    for required in ["code_snapshot_embeddings", "embedding", "snapshot_id"] {
        assert!(code_migration.contains(required), "missing {required}");
    }
    let index_migration = include_str!("../migrations/0113_pgvector_ann_indexes.sql");
    assert!(index_migration.contains("vector(1536)"));
    assert!(index_migration.contains("USING hnsw"));
    let lease_migration = include_str!("../migrations/0114_knowledge_processing_leases.sql");
    for required in ["lease_owner", "lease_token_digest", "lease_until"] {
        assert!(lease_migration.contains(required), "missing {required}");
    }
}

#[test]
fn rag_similarity_query_filters_embeddings_to_query_dimension() {
    let query = include_str!("../src/knowledge_context_search.sql");
    assert!(query.contains("e.dimension=vector_dims(($4::text)::vector)"));
    assert_eq!(
        query
            .matches("e.dimension=vector_dims(($4::text)::vector)")
            .count(),
        2
    );
    assert!(query.contains("e.embedding_model=$5"));
    assert!(!query.contains("FROM knowledge_contributions"));
    assert!(!query.contains("FROM project_documents"));
    assert!(!query.contains("FROM organization_documents"));
}

#[test]
fn rag_processing_propagates_project_scope_to_chunks() {
    let source = include_str!("../src/knowledge_rag_store.rs");
    assert!(source.contains("SELECT id,project_id,source_kind"));
    assert!(source.contains("knowledge_chunks(id,job_id,project_id,source_kind"));
    assert!(source.contains("&project_id, &source_kind"));
}

#[test]
fn rag_embedding_calls_are_outside_database_transactions() {
    let source = include_str!("../src/knowledge_rag_store.rs");
    let code_section = source
        .split("pub fn index_code_snapshot_embeddings")
        .nth(1)
        .expect("code embedding method");
    let transaction = code_section
        .find("self.client.transaction()")
        .expect("code transaction");
    let embedding = code_section.find(".embed").expect("code embedding");
    assert!(embedding < transaction);
    assert!(code_section.contains("chunks(64)"));
    assert!(code_section.contains("embed_many"));

    let knowledge_section = source
        .split("pub fn publish_knowledge_embeddings")
        .nth(1)
        .expect("knowledge embedding method");
    let transaction = knowledge_section
        .find("self.client.transaction()")
        .expect("knowledge transaction");
    let embedding = knowledge_section
        .find(".embed")
        .expect("knowledge embedding");
    assert!(embedding < transaction);
    assert!(knowledge_section.contains("chunks(64)"));
    assert!(knowledge_section.contains("embed_many"));
    assert!(code_section.contains("published) VALUES"));
    assert!(code_section.contains("embedding,published) VALUES"));
    assert!(code_section.contains(",false)"));
    assert!(source.contains("lease_token"));
    assert!(source.contains("lease_until>clock_timestamp()"));
}

#[test]
fn rag_migrations_can_resolve_the_shared_pgvector_schema() {
    let source = include_str!("../src/platform_store.rs");
    assert!(source.contains("search_path={namespace},nexora_platform,pg_catalog"));
    assert!(source.contains("CREATE EXTENSION IF NOT EXISTS vector SCHEMA nexora_platform"));
}
