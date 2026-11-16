-- Hierarchy is independent of prerequisites and execution workflow.
ALTER TABLE task_dependencies ALTER COLUMN project_id DROP NOT NULL;
ALTER TABLE task_dependencies ADD CONSTRAINT dependency_source_identity FOREIGN KEY(source_task_id) REFERENCES tasks(id);
ALTER TABLE task_dependencies ADD CONSTRAINT dependency_target_identity FOREIGN KEY(target_task_id) REFERENCES tasks(id);
CREATE TABLE task_subtasks (
 id text PRIMARY KEY, project_id text REFERENCES projects(id),
 parent_task_id text NOT NULL REFERENCES tasks(id), child_task_id text NOT NULL REFERENCES tasks(id),
 created_by text NOT NULL REFERENCES actors(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 removed_at timestamptz, removed_by text REFERENCES actors(id), removal_reason text,
 CHECK(parent_task_id<>child_task_id)
);
CREATE UNIQUE INDEX task_subtasks_one_parent ON task_subtasks(child_task_id) WHERE removed_at IS NULL;
CREATE INDEX task_subtasks_parent ON task_subtasks(parent_task_id) WHERE removed_at IS NULL;
CREATE VIEW task_wait_edges AS
 SELECT parent_task_id AS waiting,child_task_id AS prerequisite FROM task_subtasks WHERE removed_at IS NULL
 UNION
 SELECT CASE WHEN kind='depends_on' THEN source_task_id ELSE target_task_id END,
        CASE WHEN kind='depends_on' THEN target_task_id ELSE source_task_id END
 FROM task_dependencies WHERE status='active' AND removed_at IS NULL AND kind IN ('blocks','depends_on');
CREATE FUNCTION task_relations_lock() RETURNS void LANGUAGE sql AS $$
 SELECT pg_advisory_xact_lock(hashtextextended(current_schema() || ':task-relations',0));
$$;
CREATE FUNCTION validate_task_relation_edge() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE left_id text; right_id text; target_id text; v_blocker_task text;
BEGIN
 PERFORM task_relations_lock();
 IF TG_TABLE_NAME='task_subtasks' THEN left_id=NEW.parent_task_id;right_id=NEW.child_task_id;
 ELSE left_id=NEW.source_task_id;right_id=NEW.target_task_id; END IF;
 IF NOT EXISTS(SELECT 1 FROM tasks a JOIN tasks b ON b.id=right_id WHERE a.id=left_id AND a.project_id IS NOT DISTINCT FROM NEW.project_id AND b.project_id IS NOT DISTINCT FROM NEW.project_id) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='task relation scope differs from task';
 END IF;
 IF NEW.removed_at IS NOT NULL THEN RETURN NEW; END IF;
 IF TG_TABLE_NAME='task_subtasks' THEN
  IF EXISTS(SELECT 1 FROM tasks p JOIN tasks c ON c.id=right_id WHERE p.id=left_id AND p.lifecycle='completed' AND c.lifecycle<>'completed') THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='completed parent cannot have unfinished subtasks';
  END IF;
 ELSE
  IF NEW.kind='related_to' THEN RETURN NEW; END IF;
  target_id=CASE WHEN NEW.kind='depends_on' THEN left_id ELSE right_id END;
  v_blocker_task=CASE WHEN NEW.kind='depends_on' THEN right_id ELSE left_id END;
  IF EXISTS(SELECT 1 FROM tasks t JOIN tasks b ON b.id=v_blocker_task WHERE t.id=target_id AND t.lifecycle<>'pending' AND b.lifecycle<>'completed') THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='cannot add unfinished prerequisite to started task';
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER task_subtask_scope BEFORE INSERT OR UPDATE ON task_subtasks FOR EACH ROW EXECUTE FUNCTION validate_task_relation_edge();
CREATE TRIGGER task_dependency_scope BEFORE INSERT OR UPDATE ON task_dependencies FOR EACH ROW EXECUTE FUNCTION validate_task_relation_edge();
CREATE FUNCTION validate_task_relation_cycle() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(WITH RECURSIVE reach(start,id) AS (SELECT waiting,prerequisite FROM task_wait_edges UNION SELECT r.start,e.prerequisite FROM reach r JOIN task_wait_edges e ON e.waiting=r.id) SELECT 1 FROM reach WHERE start=id) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='task relations would create a cycle';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER task_subtask_cycle AFTER INSERT OR UPDATE ON task_subtasks FOR EACH ROW EXECUTE FUNCTION validate_task_relation_cycle();
CREATE TRIGGER task_dependency_cycle AFTER INSERT OR UPDATE ON task_dependencies FOR EACH ROW EXECUTE FUNCTION validate_task_relation_cycle();
CREATE FUNCTION enforce_task_relation_execution() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='INSERT' THEN RETURN NEW; END IF;
 IF NEW.lifecycle=OLD.lifecycle THEN RETURN NEW; END IF;
 PERFORM task_relations_lock();
 IF NEW.lifecycle='completed' AND EXISTS(SELECT 1 FROM task_subtasks s JOIN tasks c ON c.id=s.child_task_id WHERE s.parent_task_id=NEW.id AND s.removed_at IS NULL AND c.lifecycle<>'completed') THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='complete all subtasks before completing this task';
 END IF;
 IF NEW.lifecycle<>'completed' AND EXISTS(SELECT 1 FROM task_subtasks s JOIN tasks p ON p.id=s.parent_task_id WHERE s.child_task_id=NEW.id AND s.removed_at IS NULL AND p.lifecycle='completed') THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='cannot reopen subtask of completed parent';
 END IF;
 IF TG_OP='UPDATE' AND OLD.lifecycle='pending' AND NEW.lifecycle<>'pending' AND EXISTS(SELECT 1 FROM task_dependencies d JOIN tasks b ON b.id=CASE WHEN d.kind='depends_on' THEN d.target_task_id ELSE d.source_task_id END WHERE d.status='active' AND d.removed_at IS NULL AND ((d.kind='depends_on' AND d.source_task_id=NEW.id) OR(d.kind='blocks' AND d.target_task_id=NEW.id)) AND b.lifecycle<>'completed') THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='complete blocking tasks before taking this task';
 END IF;
 RETURN NEW;
END $$;
-- Runs after task_workflow_state derives lifecycle from the pinned state.
CREATE TRIGGER z_task_relation_execution BEFORE INSERT OR UPDATE ON tasks FOR EACH ROW EXECUTE FUNCTION enforce_task_relation_execution();

CREATE FUNCTION task_relation_counts(task text) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object(
  'blocking_count',(SELECT count(*) FROM task_dependencies d JOIN tasks b ON b.id=CASE WHEN d.kind='depends_on' THEN d.target_task_id ELSE d.source_task_id END WHERE d.status='active' AND d.removed_at IS NULL AND ((d.kind='depends_on' AND d.source_task_id=task)OR(d.kind='blocks' AND d.target_task_id=task)) AND b.lifecycle<>'completed'),
  'subtasks_total',(SELECT count(*) FROM task_subtasks s WHERE s.parent_task_id=task AND s.removed_at IS NULL),
  'subtasks_completed',(SELECT count(*) FROM task_subtasks s JOIN tasks c ON c.id=s.child_task_id WHERE s.parent_task_id=task AND s.removed_at IS NULL AND c.lifecycle='completed'));
$$;
