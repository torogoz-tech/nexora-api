CREATE TABLE repository_development_policies (
    repository_id text PRIMARY KEY REFERENCES project_repositories(id),
    project_id text NOT NULL REFERENCES projects(id),
    base_branch text NOT NULL,
    branch_prefix text NOT NULL,
    revision bigint NOT NULL CHECK(revision>0)
);
CREATE TABLE task_work_sessions (
    id text PRIMARY KEY,
    project_id text NOT NULL REFERENCES projects(id),
    task_id text NOT NULL REFERENCES tasks(id),
    actor_id text NOT NULL REFERENCES actors(id),
    task_revision bigint NOT NULL CHECK(task_revision>0),
    workflow_version_id text NOT NULL REFERENCES workflow_versions(id),
    transition_id text NOT NULL,
    reported boolean NOT NULL DEFAULT false,
    record jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX task_work_transition ON task_work_sessions(project_id,task_id,actor_id,task_revision,transition_id) WHERE reported;
