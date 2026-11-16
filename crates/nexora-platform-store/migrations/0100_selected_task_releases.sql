CREATE TABLE task_releases (
 id text PRIMARY KEY,project_id text NOT NULL REFERENCES projects(id),version text NOT NULL,branch_name text NOT NULL,
 status text NOT NULL DEFAULT 'prepared' CHECK(status IN ('prepared','published')),revision bigint NOT NULL DEFAULT 1,
 created_by text NOT NULL REFERENCES actors(id),created_at timestamptz NOT NULL DEFAULT clock_timestamp(),published_at timestamptz,publication_evidence jsonb,
 UNIQUE(project_id,version),UNIQUE(project_id,branch_name)
);
CREATE TABLE task_release_items (
 release_id text NOT NULL REFERENCES task_releases(id),task_id text NOT NULL REFERENCES tasks(id),task_revision bigint NOT NULL,included_by_parent_id text REFERENCES tasks(id),
 PRIMARY KEY(release_id,task_id)
);
CREATE TABLE task_release_repositories (
 release_id text NOT NULL REFERENCES task_releases(id),repository_id text NOT NULL REFERENCES project_repositories(id),head_sha text NOT NULL,
 PRIMARY KEY(release_id,repository_id)
);
CREATE TABLE task_release_source_pins (
 release_id text NOT NULL,task_id text NOT NULL,repository_id text NOT NULL REFERENCES project_repositories(id),source_head_sha text NOT NULL,
 PRIMARY KEY(release_id,task_id,repository_id),FOREIGN KEY(release_id,task_id) REFERENCES task_release_items(release_id,task_id)
);
