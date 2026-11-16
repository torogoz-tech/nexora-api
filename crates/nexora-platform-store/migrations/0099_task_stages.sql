ALTER TABLE tasks DROP CONSTRAINT task_pool_execution;
ALTER TABLE tasks ADD CONSTRAINT task_pool_execution CHECK(
 (lifecycle='pending' AND workflow_id IS NULL AND workflow_version_id IS NULL AND state_id IS NULL)
 OR (purpose='general' AND workflow_id IS NULL AND workflow_version_id IS NULL AND state_id IS NULL)
 OR (lifecycle IN ('active','paused','completed') AND workflow_id IS NOT NULL AND workflow_version_id IS NOT NULL AND state_id IS NOT NULL));
CREATE TABLE task_stage_completions (
 task_id text NOT NULL REFERENCES tasks(id), workflow_version_id text NOT NULL REFERENCES workflow_versions(id),
 state_id text NOT NULL, task_revision bigint NOT NULL, completed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(task_id,workflow_version_id,task_revision)
);
ALTER TABLE task_subtasks ADD COLUMN stage_workflow_version_id text REFERENCES workflow_versions(id);
UPDATE task_subtasks s SET stage_workflow_version_id=p.workflow_version_id FROM tasks p WHERE p.id=s.parent_task_id;
ALTER TABLE task_subtasks ADD COLUMN work_kind text NOT NULL DEFAULT 'decomposition' CHECK(work_kind IN ('decomposition','corrective'));
ALTER TABLE task_subtasks ADD COLUMN tested_commit_sha text;
ALTER TABLE task_subtasks ADD COLUMN tested_repository_id text REFERENCES project_repositories(id);
ALTER TABLE task_subtasks ADD CONSTRAINT corrective_tested_revision CHECK(work_kind<>'corrective' OR (tested_commit_sha IS NOT NULL AND tested_repository_id IS NOT NULL));
CREATE OR REPLACE FUNCTION validate_task_workflow_state() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE terminal_state boolean;
BEGIN
 IF NEW.workflow_id IS NULL THEN
  IF NEW.purpose<>'general' AND NEW.lifecycle<>'pending' THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='workflow work requires a pinned workflow';
  END IF;
  RETURN NEW;
 END IF;
 SELECT COALESCE((s->>'terminal')::boolean,false) INTO terminal_state
 FROM workflow_versions v,jsonb_array_elements(v.graph->'states') s
 WHERE v.id=NEW.workflow_version_id AND v.workflow_id=NEW.workflow_id
 AND (v.project_id IS NULL OR v.project_id=NEW.project_id)
 AND v.resource_kinds @> '["task"]'::jsonb AND s->>'id'=NEW.state_id;
 IF terminal_state IS NULL THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='task state outside pinned workflow';
 END IF;
 IF NOT NEW.governed_stage THEN
  IF NEW.lifecycle<>'paused' THEN NEW.lifecycle=CASE WHEN terminal_state THEN 'completed' ELSE 'active' END; END IF;
 ELSE
  IF NEW.lifecycle='completed' AND (TG_OP='INSERT' OR OLD.lifecycle<>'completed') AND NOT EXISTS(
    SELECT 1 FROM task_production_publications WHERE task_id=NEW.id AND task_revision=NEW.revision-1
  ) THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='workflow task completes only after verified production publication'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TABLE task_production_publications (
 task_id text NOT NULL REFERENCES tasks(id), task_revision bigint NOT NULL,
 release_id text NOT NULL, evidence jsonb NOT NULL,
 published_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(task_id,task_revision)
);
CREATE FUNCTION record_task_stage_completion() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.governed_stage AND NEW.workflow_version_id IS NOT NULL AND EXISTS(
  SELECT 1 FROM workflow_versions v,jsonb_array_elements(v.graph->'states') s
  WHERE v.id=NEW.workflow_version_id AND s->>'id'=NEW.state_id AND COALESCE((s->>'terminal')::boolean,false)
 ) AND (TG_OP='INSERT' OR OLD.state_id IS DISTINCT FROM NEW.state_id OR OLD.workflow_version_id IS DISTINCT FROM NEW.workflow_version_id) THEN
  INSERT INTO task_stage_completions(task_id,workflow_version_id,state_id,task_revision)
  VALUES(NEW.id,NEW.workflow_version_id,NEW.state_id,NEW.revision) ON CONFLICT DO NOTHING;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER task_stage_completed AFTER INSERT OR UPDATE ON tasks FOR EACH ROW EXECUTE FUNCTION record_task_stage_completion();
CREATE FUNCTION pin_subtask_stage() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.workflow_version_id IS NOT NULL AND (TG_OP='INSERT' OR OLD.workflow_version_id IS DISTINCT FROM NEW.workflow_version_id) THEN
  UPDATE task_subtasks SET stage_workflow_version_id=NEW.workflow_version_id
  WHERE parent_task_id=NEW.id AND removed_at IS NULL AND stage_workflow_version_id IS NULL;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER pin_subtask_stage AFTER INSERT OR UPDATE ON tasks FOR EACH ROW EXECUTE FUNCTION pin_subtask_stage();
CREATE TABLE task_repository_branches (
 task_id text NOT NULL REFERENCES tasks(id), repository_id text NOT NULL REFERENCES project_repositories(id),
 branch_name text NOT NULL, base_branch text NOT NULL, base_sha text NOT NULL, head_sha text NOT NULL,
 evidence jsonb NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(task_id,repository_id)
);
