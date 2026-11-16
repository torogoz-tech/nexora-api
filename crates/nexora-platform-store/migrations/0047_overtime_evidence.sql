CREATE TABLE overtime_task_links(
 request_id text NOT NULL REFERENCES labor_overtime_requests(id),
 task_id text NOT NULL REFERENCES tasks(id),
 added_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(request_id,task_id)
);
CREATE TABLE overtime_commit_links(
 request_id text NOT NULL REFERENCES labor_overtime_requests(id),
 source_event_id text NOT NULL REFERENCES repository_source_events(id),
 added_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(request_id,source_event_id)
);
CREATE FUNCTION overtime_evidence_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='overtime evidence is append-only';
END
$$;
CREATE TRIGGER overtime_tasks_immutable BEFORE UPDATE OR DELETE ON overtime_task_links FOR EACH ROW EXECUTE FUNCTION overtime_evidence_immutable();
CREATE TRIGGER overtime_commits_immutable BEFORE UPDATE OR DELETE ON overtime_commit_links FOR EACH ROW EXECUTE FUNCTION overtime_evidence_immutable();
