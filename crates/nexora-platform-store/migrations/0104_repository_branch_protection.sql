CREATE TABLE repository_branch_protection (
 repository_id text PRIMARY KEY REFERENCES project_repositories(id),
 project_id text NOT NULL REFERENCES projects(id),
 revision bigint NOT NULL CHECK(revision>0),
 status text NOT NULL CHECK(status IN ('prepared','applying','applied','failed')),
 required_checks jsonb NOT NULL,
 authority jsonb NOT NULL,
 plan jsonb NOT NULL,
 result jsonb,
 requested_by text NOT NULL REFERENCES actors(id),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
