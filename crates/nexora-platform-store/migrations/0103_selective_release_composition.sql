CREATE TABLE task_commit_links (
 id text PRIMARY KEY,task_id text NOT NULL REFERENCES tasks(id),repository_id text NOT NULL REFERENCES project_repositories(id),
 commit_sha text NOT NULL,sequence bigint NOT NULL,provider_evidence jsonb NOT NULL,actor_id text NOT NULL REFERENCES actors(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),removed_at timestamptz,
 UNIQUE(task_id,repository_id,commit_sha)
);
CREATE TABLE task_release_commit_pins (
 release_id text NOT NULL REFERENCES task_releases(id),task_id text NOT NULL REFERENCES tasks(id),repository_id text NOT NULL REFERENCES project_repositories(id),commit_sha text NOT NULL,sequence bigint NOT NULL,
 PRIMARY KEY(release_id,task_id,repository_id,commit_sha)
);
ALTER TABLE task_release_repositories ADD COLUMN composition_status text NOT NULL DEFAULT 'uncomposed' CHECK(composition_status IN ('uncomposed','composing','composed','failed'));
ALTER TABLE task_release_repositories ADD COLUMN composition_evidence jsonb;
ALTER TABLE task_release_repositories ALTER COLUMN head_sha DROP NOT NULL;
CREATE TABLE task_release_composition_jobs (
 id text PRIMARY KEY,release_id text NOT NULL REFERENCES task_releases(id),project_id text NOT NULL REFERENCES projects(id),release_revision bigint NOT NULL,
 actor_id text NOT NULL REFERENCES actors(id),credential_id text NOT NULL,auth_method text NOT NULL CHECK(auth_method IN ('session','api_key')),
 status text NOT NULL DEFAULT 'queued' CHECK(status IN ('queued','running','succeeded','failed')),record jsonb NOT NULL DEFAULT '{}',
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(release_id,release_revision)
);
