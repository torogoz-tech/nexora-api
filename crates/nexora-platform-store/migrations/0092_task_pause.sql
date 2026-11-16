ALTER TABLE tasks DROP CONSTRAINT tasks_lifecycle_check;
ALTER TABLE tasks ADD CONSTRAINT tasks_lifecycle_check CHECK(lifecycle IN ('pending','active','paused','completed'));
ALTER TABLE tasks DROP CONSTRAINT task_pool_execution;
ALTER TABLE tasks ADD CONSTRAINT task_pool_execution CHECK(
 (lifecycle='pending' AND workflow_id IS NULL AND workflow_version_id IS NULL AND state_id IS NULL)
 OR (lifecycle IN ('active','paused','completed') AND workflow_id IS NOT NULL AND workflow_version_id IS NOT NULL AND state_id IS NOT NULL));
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
 IF NEW.lifecycle='paused' AND terminal_state THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='completed tasks cannot be paused';
 END IF;
 IF NEW.lifecycle<>'paused' THEN
  NEW.lifecycle=CASE WHEN terminal_state THEN 'completed' ELSE 'active' END;
 END IF;
 RETURN NEW;
END $$;
CREATE OR REPLACE FUNCTION enforce_developer_workflow_membership() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE member_id text; member_role text;
BEGIN
 member_id=CASE WHEN TG_OP='DELETE' THEN OLD.actor_id ELSE NEW.actor_id END;
 PERFORM id FROM actors WHERE id=member_id FOR UPDATE;
 IF TG_OP<>'DELETE' THEN
  SELECT role INTO member_role FROM actor_roles WHERE actor_id=member_id;
  IF member_role IS NULL OR member_role NOT IN ('developer','technical_lead','pmo','manager','super_admin') THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='workflow membership requires a team role';
  END IF;
  IF NOT EXISTS(SELECT 1 FROM workflow_versions v JOIN workflows w ON w.id=v.workflow_id WHERE v.id=NEW.workflow_version_id AND w.id=NEW.workflow_id AND w.project_id IS NULL AND NOT w.archived) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='member requires a published global task workflow';
  END IF;
  IF member_role='developer' AND EXISTS(SELECT 1 FROM developer_workflows d WHERE d.actor_id=member_id AND d.workflow_id<>NEW.workflow_id) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='developer can belong to only one workflow';
  END IF;
 END IF;
 IF TG_OP='DELETE' OR (TG_OP='UPDATE' AND (OLD.actor_id<>NEW.actor_id OR OLD.workflow_id<>NEW.workflow_id)) THEN
  IF EXISTS(SELECT 1 FROM tasks t WHERE t.assignee_id=OLD.actor_id AND t.workflow_id=OLD.workflow_id AND NOT t.archived AND t.lifecycle IN ('active','paused')) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='reassign active tasks before removing workflow membership';
  END IF;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
END $$;

