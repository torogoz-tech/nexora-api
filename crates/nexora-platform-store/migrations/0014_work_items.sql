CREATE TABLE task_types (
 project_id text NOT NULL REFERENCES projects(id),
 code text NOT NULL CHECK(code ~ '^[a-z0-9_-]{1,64}$'),
 name text NOT NULL CHECK(octet_length(name) BETWEEN 1 AND 160),
 description text CHECK(octet_length(description)<=2000),
 archived boolean NOT NULL DEFAULT false,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 PRIMARY KEY(project_id,code)
);
CREATE TABLE backlogs (
 id text PRIMARY KEY,
 project_id text NOT NULL REFERENCES projects(id),
 name text NOT NULL CHECK(octet_length(name) BETWEEN 1 AND 160),
 description text CHECK(octet_length(description)<=2000),
 workflow_id text NOT NULL,
 workflow_version_id text NOT NULL,
 archived boolean NOT NULL DEFAULT false,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 created_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(id,project_id),
 FOREIGN KEY(workflow_version_id,workflow_id,project_id) REFERENCES workflow_versions(id,workflow_id,project_id)
);
CREATE INDEX backlogs_project_page ON backlogs(project_id,id);
CREATE TABLE tasks (
 id text PRIMARY KEY,
 project_id text NOT NULL REFERENCES projects(id),
 title text NOT NULL CHECK(octet_length(title) BETWEEN 1 AND 256),
 context text NOT NULL CHECK(octet_length(context)<=8000),
 type_code text NOT NULL,
 priority text NOT NULL CHECK(priority IN ('low','medium','high','critical')),
 assignee_id text REFERENCES actors(id),
 backlog_id text,
 workflow_id text NOT NULL,
 workflow_version_id text NOT NULL,
 state_id text NOT NULL CHECK(state_id ~ '^[A-Za-z0-9_-]{1,64}$'),
 origin text NOT NULL CHECK(octet_length(origin) BETWEEN 1 AND 128),
 origin_reference text CHECK(octet_length(origin_reference)<=2048),
 resolution text CHECK(octet_length(resolution)<=4000),
 blocked boolean NOT NULL DEFAULT false,
 block_reason text CHECK(octet_length(block_reason)<=2000),
 blocker_id text REFERENCES actors(id),
 fields jsonb NOT NULL DEFAULT '{}' CHECK(jsonb_typeof(fields)='object' AND octet_length(fields::text)<=32768),
 position bigint,
 archived boolean NOT NULL DEFAULT false,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 history_sequence bigint NOT NULL DEFAULT 0 CHECK(history_sequence>=0),
 created_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(id,project_id),
 UNIQUE(backlog_id,position) DEFERRABLE INITIALLY DEFERRED,
 CHECK((backlog_id IS NULL)=(position IS NULL)),
 CHECK((blocked AND block_reason IS NOT NULL AND octet_length(block_reason)>0 AND blocker_id IS NOT NULL) OR (NOT blocked AND block_reason IS NULL AND blocker_id IS NULL)),
 FOREIGN KEY(project_id,type_code) REFERENCES task_types(project_id,code),
 FOREIGN KEY(backlog_id,project_id) REFERENCES backlogs(id,project_id),
 FOREIGN KEY(workflow_version_id,workflow_id,project_id) REFERENCES workflow_versions(id,workflow_id,project_id)
);
CREATE INDEX tasks_project_page ON tasks(project_id,id);
CREATE INDEX tasks_backlog_page ON tasks(project_id,backlog_id,position,id);
CREATE INDEX tasks_assignee ON tasks(project_id,assignee_id,id);
CREATE TABLE task_history (
 id text PRIMARY KEY,
 project_id text NOT NULL,
 task_id text NOT NULL,
 sequence bigint NOT NULL CHECK(sequence>0),
 revision bigint NOT NULL CHECK(revision>0),
 operation text NOT NULL,
 actor_id text NOT NULL REFERENCES actors(id),
 credential_id text NOT NULL,
 details jsonb NOT NULL CHECK(octet_length(details::text)<=131072),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(task_id,project_id) REFERENCES tasks(id,project_id),
 UNIQUE(task_id,sequence)
);
CREATE TABLE task_dependencies (
 id text PRIMARY KEY,
 project_id text NOT NULL REFERENCES projects(id),
 source_task_id text NOT NULL,
 target_task_id text NOT NULL,
 kind text NOT NULL CHECK(kind IN ('blocks','depends_on','related_to')),
 status text NOT NULL DEFAULT 'active' CHECK(status IN ('active','resolved')),
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 created_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 CHECK(source_task_id<>target_task_id),
 FOREIGN KEY(source_task_id,project_id) REFERENCES tasks(id,project_id),
 FOREIGN KEY(target_task_id,project_id) REFERENCES tasks(id,project_id)
);
CREATE UNIQUE INDEX task_dependency_active ON task_dependencies(project_id,source_task_id,target_task_id,kind) WHERE status='active';
CREATE INDEX task_dependencies_source ON task_dependencies(project_id,source_task_id,status);
CREATE INDEX task_dependencies_target ON task_dependencies(project_id,target_task_id,status);
CREATE FUNCTION validate_task_workflow_state() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM workflow_versions v WHERE v.id=NEW.workflow_version_id AND v.workflow_id=NEW.workflow_id AND v.project_id=NEW.project_id AND v.resource_kinds @> '["task"]'::jsonb AND EXISTS(SELECT 1 FROM jsonb_array_elements(v.graph->'states') s WHERE s->>'id'=NEW.state_id)) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='task state outside pinned workflow';
 END IF;
 RETURN NEW;
END;
$$;
CREATE TRIGGER task_workflow_state BEFORE INSERT OR UPDATE OF workflow_id,workflow_version_id,project_id,state_id ON tasks
 FOR EACH ROW EXECUTE FUNCTION validate_task_workflow_state();
