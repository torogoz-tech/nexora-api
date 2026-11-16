CREATE INDEX tasks_active_assignee ON tasks(assignee_id) WHERE lifecycle='active' AND NOT archived;
CREATE FUNCTION enforce_single_active_task() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.lifecycle='active' AND NOT NEW.archived AND NEW.assignee_id IS NOT NULL THEN
  PERFORM id FROM actors WHERE id=NEW.assignee_id FOR UPDATE;
  IF EXISTS(SELECT 1 FROM actor_roles WHERE actor_id=NEW.assignee_id AND role IN ('developer','technical_lead'))
   AND EXISTS(SELECT 1 FROM tasks WHERE assignee_id=NEW.assignee_id AND lifecycle='active' AND NOT archived AND id<>NEW.id) THEN
   RAISE EXCEPTION USING ERRCODE='23514',CONSTRAINT='task_single_active',MESSAGE='pause the active task before starting another';
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER zz_task_single_active BEFORE INSERT OR UPDATE ON tasks FOR EACH ROW EXECUTE FUNCTION enforce_single_active_task();
CREATE FUNCTION enforce_single_active_task_role() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM id FROM actors WHERE id=NEW.actor_id FOR UPDATE;
 IF NEW.role IN ('developer','technical_lead') AND (SELECT count(*) FROM tasks WHERE assignee_id=NEW.actor_id AND lifecycle='active' AND NOT archived)>1 THEN
  RAISE EXCEPTION USING ERRCODE='23514',CONSTRAINT='task_single_active',MESSAGE='pause additional tasks before assigning this role';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER actor_single_active_task BEFORE INSERT OR UPDATE ON actor_roles FOR EACH ROW EXECUTE FUNCTION enforce_single_active_task_role();

ALTER TABLE project_features ADD COLUMN status text NOT NULL DEFAULT 'active' CHECK(status IN ('active','completed'));
ALTER TABLE project_features ADD COLUMN completed_at timestamptz;
ALTER TABLE project_features ADD COLUMN completed_by text REFERENCES actors(id);
ALTER TABLE project_features ADD CONSTRAINT feature_completion_record CHECK((status='active' AND completed_at IS NULL AND completed_by IS NULL) OR (status='completed' AND completed_at IS NOT NULL AND completed_by IS NOT NULL));
