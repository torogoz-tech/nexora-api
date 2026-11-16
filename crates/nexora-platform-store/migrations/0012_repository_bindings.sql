ALTER TABLE project_repositories ADD CONSTRAINT repository_project_identity UNIQUE(id,project_id);
CREATE TABLE repository_bindings(
 repository_id text PRIMARY KEY,
 project_id text NOT NULL,
 runtime_tenant_id text NOT NULL REFERENCES installation(id),
 runtime_repository_id text NOT NULL CHECK(runtime_repository_id=repository_id),
 snapshot_id text NOT NULL,
 payload_hash text NOT NULL CHECK(payload_hash ~ '^[0-9a-f]{64}$'),
 pinned_commit text NOT NULL CHECK(pinned_commit ~ '^([0-9a-f]{40}|[0-9a-f]{64})$'),
 source_generation bigint NOT NULL CHECK(source_generation>=0),
 repository_revision bigint NOT NULL CHECK(repository_revision>0),
 connection_id text NOT NULL REFERENCES git_connections(id),
 connection_revision bigint NOT NULL CHECK(connection_revision>0),
 job_id text NOT NULL REFERENCES repository_jobs(id),
 attempt integer NOT NULL CHECK(attempt>0),
 completion_digest bytea NOT NULL CHECK(octet_length(completion_digest)=32),
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 published_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(repository_id,project_id) REFERENCES project_repositories(id,project_id),
 UNIQUE(runtime_tenant_id,runtime_repository_id)
);
CREATE INDEX repository_binding_project ON repository_bindings(project_id,repository_id);
