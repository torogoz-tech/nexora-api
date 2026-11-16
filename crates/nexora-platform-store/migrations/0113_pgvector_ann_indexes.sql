CREATE INDEX knowledge_embeddings_vector_1536 ON knowledge_embeddings
 USING hnsw ((embedding::vector(1536)) vector_cosine_ops)
 WHERE published AND dimension=1536;
CREATE INDEX code_snapshot_embeddings_vector_1536 ON code_snapshot_embeddings
 USING hnsw ((embedding::vector(1536)) vector_cosine_ops)
 WHERE published AND dimension=1536;
