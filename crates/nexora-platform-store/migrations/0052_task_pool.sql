-- Tasks exist independently of an execution workflow or project.
ALTER TABLE tasks ALTER COLUMN project_id DROP NOT NULL;
ALTER TABLE tasks ALTER COLUMN workflow_id DROP NOT NULL;
ALTER TABLE tasks ALTER COLUMN workflow_version_id DROP NOT NULL;
ALTER TABLE tasks ALTER COLUMN state_id DROP NOT NULL;
ALTER TABLE tasks ADD COLUMN lifecycle text NOT NULL DEFAULT 'pending'
 CHECK(lifecycle IN ('pending','active','completed'));
UPDATE tasks t SET lifecycle=CASE WHEN EXISTS(
 SELECT 1 FROM workflow_versions v,jsonb_array_elements(v.graph->'states') s
 WHERE v.id=t.workflow_version_id AND s->>'id'=t.state_id AND (s->>'terminal')::boolean
) THEN 'completed' ELSE 'active' END;
ALTER TABLE tasks ADD CONSTRAINT task_pool_execution CHECK(
 (lifecycle='pending' AND workflow_id IS NULL AND workflow_version_id IS NULL AND state_id IS NULL)
 OR (lifecycle IN ('active','completed') AND workflow_id IS NOT NULL AND workflow_version_id IS NOT NULL AND state_id IS NOT NULL));
ALTER TABLE tasks ADD CONSTRAINT projectless_task_backlog CHECK(project_id IS NOT NULL OR backlog_id IS NULL);
ALTER TABLE tasks ADD CONSTRAINT task_catalog_type FOREIGN KEY(type_code) REFERENCES task_type_catalog(code);
ALTER TABLE task_history ALTER COLUMN project_id DROP NOT NULL;
ALTER TABLE task_comments ALTER COLUMN project_id DROP NOT NULL;
ALTER TABLE task_approvals ALTER COLUMN project_id DROP NOT NULL;
-- Composite foreign keys skip NULL scope; these references keep the task identity mandatory.
ALTER TABLE task_history ADD CONSTRAINT history_task_identity FOREIGN KEY(task_id) REFERENCES tasks(id);
ALTER TABLE task_comments ADD CONSTRAINT comment_task_identity FOREIGN KEY(task_id) REFERENCES tasks(id);
ALTER TABLE task_approvals ADD CONSTRAINT approval_task_identity FOREIGN KEY(task_id) REFERENCES tasks(id);
CREATE INDEX tasks_pool_lifecycle ON tasks(lifecycle,id);
CREATE OR REPLACE FUNCTION validate_task_workflow_state() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE terminal_state boolean;
BEGIN
 IF NEW.lifecycle='pending' THEN RETURN NEW; END IF;
 SELECT COALESCE((s->>'terminal')::boolean,false) INTO terminal_state
 FROM workflow_versions v,jsonb_array_elements(v.graph->'states') s
 WHERE v.id=NEW.workflow_version_id AND v.workflow_id=NEW.workflow_id
 AND (v.project_id IS NULL OR v.project_id=NEW.project_id)
 AND v.resource_kinds @> '["task"]'::jsonb AND s->>'id'=NEW.state_id;
 IF terminal_state IS NULL THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='task state outside pinned workflow';
 END IF;
 NEW.lifecycle=CASE WHEN terminal_state THEN 'completed' ELSE 'active' END;
 RETURN NEW;
END $$;
DROP TRIGGER task_workflow_state ON tasks;
CREATE TRIGGER task_workflow_state BEFORE INSERT OR UPDATE ON tasks
 FOR EACH ROW EXECUTE FUNCTION validate_task_workflow_state();
-- Reservation has no dependency on published workflows. Active tasks retain their pinned graph.
DROP TRIGGER developer_task_workflow ON tasks;
CREATE FUNCTION validate_task_contribution_scope() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM tasks t WHERE t.id=NEW.task_id AND t.project_id IS NOT DISTINCT FROM NEW.project_id) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='task contribution scope differs from task';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER history_task_scope BEFORE INSERT ON task_history FOR EACH ROW EXECUTE FUNCTION validate_task_contribution_scope();
CREATE TRIGGER comment_task_scope BEFORE INSERT ON task_comments FOR EACH ROW EXECUTE FUNCTION validate_task_contribution_scope();
CREATE TRIGGER approval_task_scope BEFORE INSERT OR UPDATE ON task_approvals FOR EACH ROW EXECUTE FUNCTION validate_task_contribution_scope();

CREATE OR REPLACE FUNCTION enforce_developer_workflow_membership() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE developer text; BEGIN
 developer=CASE WHEN TG_OP='DELETE' THEN OLD.actor_id ELSE NEW.actor_id END;
 PERFORM id FROM actors WHERE id=developer FOR UPDATE;
 IF TG_OP<>'DELETE' AND NOT EXISTS(SELECT 1 FROM workflow_versions v JOIN workflows w ON w.id=v.workflow_id WHERE v.id=NEW.workflow_version_id AND w.id=NEW.workflow_id AND w.project_id IS NULL AND NOT w.archived) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='developer requires a published global task workflow';
 END IF;
 IF EXISTS(SELECT 1 FROM tasks t JOIN workflows w ON w.id=t.workflow_id WHERE t.assignee_id=developer AND NOT t.archived AND t.lifecycle='active' AND w.project_id IS NULL AND (TG_OP='DELETE' OR t.workflow_id<>NEW.workflow_id)) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='reassign existing tasks before changing developer workflow';
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
END $$;
