use nexora_retrieval::{KnowledgeChunker, KnowledgeKind, RagCandidate, RagFusion};

#[test]
fn chunks_are_stable_and_keep_knowledge_identity() {
    let chunker = KnowledgeChunker::new(128, 4);
    let chunks = chunker.chunk(
        KnowledgeKind::Requirement,
        "req-1",
        3,
        "Aprobación de conocimiento",
        "El requisito debe conservar evidencia y relaciones verificables.",
    );

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].knowledge_id, "req-1");
    assert_eq!(chunks[0].revision, 3);
    assert!(!chunks[0].content_hash.is_empty());
    assert_ne!(chunks[0].content_hash, chunks[0].id);
}

#[test]
fn fusion_deduplicates_and_prefers_multiple_evidence_channels() {
    let fusion = RagFusion::default();
    let results = fusion.rank(
        "aprobación",
        vec![
            RagCandidate::new("k1", 0.8, 0.1, 0.0, 0.0),
            RagCandidate::new("k1", 0.2, 0.9, 0.7, 0.4),
            RagCandidate::new("k2", 0.7, 0.0, 0.0, 0.0),
        ],
        2,
    );

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].id, "k1");
    assert!(results[0].score > results[1].score);
}
