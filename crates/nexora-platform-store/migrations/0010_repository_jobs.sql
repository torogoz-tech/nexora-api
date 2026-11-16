CREATE TABLE repository_jobs(
 id text PRIMARY KEY,
 project_id text NOT NULL REFERENCES projects(id),
 repository_id text NOT NULL REFERENCES project_repositories(id),
 requested_by text NOT NULL REFERENCES actors(id),
 credential_id text NOT NULL,
 credential_method text NOT NULL CHECK(credential_method IN ('session','api_key')),
 kind text NOT NULL CHECK(kind IN ('sync','index')),
 state text NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','succeeded','failed','cancelled')),
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 attempt integer NOT NULL DEFAULT 0 CHECK(attempt>=0),
 repository_revision bigint NOT NULL CHECK(repository_revision>0),
 source_generation bigint NOT NULL CHECK(source_generation>=0),
 connection_id text NOT NULL REFERENCES git_connections(id),
 connection_revision bigint NOT NULL CHECK(connection_revision>0),
 pinned_commit text,
 snapshot_id text,
 error_code text,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 finished_at timestamptz,
 CHECK((state IN ('succeeded','failed','cancelled')) = (finished_at IS NOT NULL)),
 CHECK(state<>'succeeded' OR (snapshot_id IS NOT NULL AND pinned_commit IS NOT NULL))
);
CREATE UNIQUE INDEX repository_job_active ON repository_jobs(repository_id) WHERE state IN ('queued','running');
CREATE INDEX repository_job_project ON repository_jobs(project_id,id);
CREATE INDEX repository_job_pending ON repository_jobs(created_at,id) WHERE state='queued';
