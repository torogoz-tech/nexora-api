CREATE TABLE git_webhook_deliveries(
 provider text NOT NULL CHECK(provider IN ('github','gitlab')),scope_id text NOT NULL,delivery_id text NOT NULL,
 event text NOT NULL,body_digest bytea NOT NULL CHECK(octet_length(body_digest)=32),received_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(provider,scope_id,delivery_id)
);
CREATE TABLE gitlab_webhooks(
 id text PRIMARY KEY,repository_id text NOT NULL REFERENCES project_repositories(id),registered_by text NOT NULL REFERENCES actors(id),
 host text NOT NULL,remote_id text NOT NULL,signing_key jsonb NOT NULL,callback_url text NOT NULL,
 active boolean NOT NULL DEFAULT true,revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX gitlab_webhook_repository ON gitlab_webhooks(repository_id,id);
ALTER TABLE project_repositories ADD COLUMN source_generation bigint NOT NULL DEFAULT 0 CHECK(source_generation>=0);
ALTER TABLE project_repositories ADD COLUMN latest_source_commit text;
ALTER TABLE project_repositories ADD COLUMN source_changed_at timestamptz;
CREATE TABLE repository_source_events(
 id text PRIMARY KEY,repository_id text NOT NULL REFERENCES project_repositories(id),source_generation bigint NOT NULL,
 provider text NOT NULL,scope_id text NOT NULL,delivery_id text NOT NULL,source_ref text NOT NULL,source_commit text NOT NULL,received_at timestamptz NOT NULL DEFAULT now(),
 UNIQUE(provider,scope_id,delivery_id,repository_id),UNIQUE(repository_id,source_generation)
);
