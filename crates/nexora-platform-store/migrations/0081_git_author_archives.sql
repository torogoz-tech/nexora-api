CREATE TABLE git_author_archives (
 project_id text NOT NULL REFERENCES projects(id),
 provider text NOT NULL,
 host text NOT NULL,
 email text NOT NULL,
 archived_by text NOT NULL REFERENCES actors(id),
 archived_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(project_id,provider,host,email)
);
