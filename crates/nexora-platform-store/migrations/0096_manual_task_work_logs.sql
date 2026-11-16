DROP TRIGGER IF EXISTS zz_task_single_active ON tasks;
DROP TRIGGER IF EXISTS actor_single_active_task ON actor_roles;
DROP FUNCTION IF EXISTS enforce_single_active_task();
DROP FUNCTION IF EXISTS enforce_single_active_task_role();
CREATE TABLE task_work_logs (
 id text PRIMARY KEY, task_id text NOT NULL REFERENCES tasks(id), actor_id text NOT NULL REFERENCES actors(id),
 overtime_request_id text REFERENCES labor_overtime_requests(id), date date NOT NULL, minutes integer NOT NULL CHECK(minutes BETWEEN 1 AND 1440), note text CHECK(length(note)<=4000),
 revision bigint NOT NULL DEFAULT 1, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(), removed_at timestamptz
);
CREATE INDEX task_work_logs_task ON task_work_logs(task_id,date,id) WHERE removed_at IS NULL;
CREATE INDEX task_work_logs_member ON task_work_logs(actor_id,date) WHERE removed_at IS NULL;
