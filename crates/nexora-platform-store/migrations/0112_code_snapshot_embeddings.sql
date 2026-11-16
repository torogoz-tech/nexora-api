CREATE TABLE code_snapshot_embeddings (
 project_id text NOT NULL REFERENCES projects(id),
 repository_id text NOT NULL,
 snapshot_id text NOT NULL,
 path text NOT NULL,
 sequence integer NOT NULL CHECK(sequence >= 0),
 title text NOT NULL,
 content text NOT NULL CHECK(length(content) > 0),
 content_hash text NOT NULL CHECK(length(content_hash) BETWEEN 1 AND 128),
 embedding_model text NOT NULL CHECK(length(embedding_model) BETWEEN 1 AND 128),
 dimension integer NOT NULL CHECK(dimension > 0),
 embedding vector NOT NULL,
 published boolean NOT NULL DEFAULT true,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(project_id,repository_id,snapshot_id,path,sequence,embedding_model,dimension)
);
CREATE INDEX code_snapshot_embeddings_scope ON code_snapshot_embeddings(project_id,repository_id,snapshot_id,published);
CREATE INDEX code_snapshot_embeddings_text ON code_snapshot_embeddings USING gin(to_tsvector('simple',title||' '||content));
