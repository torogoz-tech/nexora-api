CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE knowledge_processing_jobs (
 id text PRIMARY KEY,
 project_id text REFERENCES projects(id),
 source_kind text NOT NULL,
 resource_id text NOT NULL,
 source_revision bigint NOT NULL CHECK(source_revision > 0),
 content_hash text NOT NULL CHECK(length(content_hash) BETWEEN 1 AND 128),
 status text NOT NULL DEFAULT 'queued' CHECK(status IN ('queued','processing','structured','indexed','failed','cancelled')),
 attempts integer NOT NULL DEFAULT 0 CHECK(attempts >= 0),
 claimed_at timestamptz,
 last_error text,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(source_kind, resource_id, source_revision, content_hash)
);
CREATE INDEX knowledge_processing_queue ON knowledge_processing_jobs(status,created_at,id);
CREATE INDEX knowledge_processing_scope ON knowledge_processing_jobs(project_id,status,id);

CREATE TABLE knowledge_structures (
 job_id text PRIMARY KEY REFERENCES knowledge_processing_jobs(id),
 source_kind text NOT NULL,
 resource_id text NOT NULL,
 source_revision bigint NOT NULL CHECK(source_revision > 0),
 schema_version text NOT NULL,
 structure jsonb NOT NULL CHECK(jsonb_typeof(structure)='object'),
 structure_hash text NOT NULL CHECK(length(structure_hash) BETWEEN 1 AND 128),
 validation_status text NOT NULL CHECK(validation_status IN ('valid','invalid')),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX knowledge_structures_resource ON knowledge_structures(source_kind,resource_id,source_revision);

CREATE TABLE knowledge_chunks (
 id text PRIMARY KEY,
 job_id text NOT NULL REFERENCES knowledge_processing_jobs(id),
 project_id text REFERENCES projects(id),
 source_kind text NOT NULL,
 resource_id text NOT NULL,
 source_revision bigint NOT NULL CHECK(source_revision > 0),
 sequence integer NOT NULL CHECK(sequence >= 0),
 title text NOT NULL,
 content text NOT NULL CHECK(length(content) > 0),
 content_hash text NOT NULL CHECK(length(content_hash) BETWEEN 1 AND 128),
 metadata jsonb NOT NULL DEFAULT '{}'::jsonb CHECK(jsonb_typeof(metadata)='object'),
 published boolean NOT NULL DEFAULT false,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(job_id,sequence)
);
CREATE INDEX knowledge_chunks_scope ON knowledge_chunks(project_id,published,source_kind,resource_id);
CREATE INDEX knowledge_chunks_text ON knowledge_chunks USING gin(to_tsvector('simple',title||' '||content));

CREATE TABLE knowledge_embeddings (
 chunk_id text NOT NULL REFERENCES knowledge_chunks(id),
 embedding_model text NOT NULL CHECK(length(embedding_model) BETWEEN 1 AND 128),
 dimension integer NOT NULL CHECK(dimension > 0),
 embedding vector NOT NULL,
 content_hash text NOT NULL CHECK(length(content_hash) BETWEEN 1 AND 128),
 published boolean NOT NULL DEFAULT false,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(chunk_id,embedding_model,dimension)
);
CREATE INDEX knowledge_embeddings_published ON knowledge_embeddings(published,embedding_model,dimension);

CREATE FUNCTION knowledge_rag_touch_updated_at() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 NEW.updated_at = clock_timestamp();
 RETURN NEW;
END $$;
CREATE TRIGGER knowledge_processing_jobs_updated BEFORE UPDATE ON knowledge_processing_jobs FOR EACH ROW EXECUTE FUNCTION knowledge_rag_touch_updated_at();
