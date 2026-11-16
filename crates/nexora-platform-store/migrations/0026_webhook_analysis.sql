ALTER TABLE repository_jobs DROP CONSTRAINT repository_jobs_credential_method_check;
ALTER TABLE repository_jobs ADD CONSTRAINT repository_jobs_credential_method_check CHECK(credential_method IN ('session','api_key','webhook'));
ALTER TABLE repository_jobs ADD COLUMN source_event_id text UNIQUE REFERENCES repository_source_events(id);
ALTER TABLE repository_jobs ADD COLUMN available_at timestamptz NOT NULL DEFAULT clock_timestamp();
ALTER TABLE repository_jobs ADD CONSTRAINT repository_job_webhook_origin CHECK((credential_method='webhook')=(source_event_id IS NOT NULL));
ALTER TABLE repository_jobs ADD CONSTRAINT repository_job_webhook_commit CHECK(credential_method<>'webhook' OR pinned_commit IS NOT NULL);
CREATE TABLE repository_analysis_requests (
 id text PRIMARY KEY REFERENCES repository_source_events(id),
 project_id text NOT NULL REFERENCES projects(id),
 repository_id text NOT NULL REFERENCES project_repositories(id),
 requested_by text NOT NULL REFERENCES actors(id),
 repository_revision bigint NOT NULL,
 source_generation bigint NOT NULL,
 connection_id text NOT NULL REFERENCES git_connections(id),
 connection_revision bigint NOT NULL,
 state text NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','queued','superseded','rejected')),
 job_id text UNIQUE REFERENCES repository_jobs(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 CHECK((state='queued')=(job_id IS NOT NULL))
);
CREATE INDEX repository_analysis_pending ON repository_analysis_requests(created_at,id) WHERE state='pending';
