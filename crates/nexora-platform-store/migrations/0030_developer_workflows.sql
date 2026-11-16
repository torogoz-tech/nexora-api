-- A task workflow is shared by developers, never owned by a project.
-- Existing document lifecycle policies keep their original project scope.
DO $$ DECLARE dependency record; BEGIN
 FOR dependency IN
  SELECT conrelid::regclass AS owner, conname FROM pg_constraint
  WHERE contype='f' AND conrelid IN ('tasks'::regclass,'backlogs'::regclass,'workflows'::regclass,'workflow_versions'::regclass)
    AND confrelid IN ('workflows'::regclass,'workflow_versions'::regclass)
 LOOP EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I', dependency.owner, dependency.conname); END LOOP;
END $$;
ALTER TABLE workflows ALTER COLUMN project_id DROP NOT NULL;
ALTER TABLE workflow_versions ALTER COLUMN project_id DROP NOT NULL;
ALTER TABLE workflow_versions ADD CONSTRAINT workflow_version_pair UNIQUE(id,workflow_id);
ALTER TABLE workflow_versions ADD CONSTRAINT workflow_version_definition_fk FOREIGN KEY(workflow_id) REFERENCES workflows(id);
ALTER TABLE workflows ADD CONSTRAINT workflow_latest_pair_fk FOREIGN KEY(latest_version_id,id) REFERENCES workflow_versions(id,workflow_id);
ALTER TABLE tasks ADD CONSTRAINT task_workflow_pair_fk FOREIGN KEY(workflow_version_id,workflow_id) REFERENCES workflow_versions(id,workflow_id);
ALTER TABLE backlogs ADD CONSTRAINT backlog_workflow_pair_fk FOREIGN KEY(workflow_version_id,workflow_id) REFERENCES workflow_versions(id,workflow_id);
-- Only scope metadata changes. Published graphs, checksums and identifiers stay intact.
ALTER TABLE workflow_versions DISABLE TRIGGER workflow_version_immutable;
UPDATE workflow_versions SET project_id=NULL WHERE workflow_id IN (SELECT id FROM workflows WHERE resource_kinds='["task"]'::jsonb);
UPDATE workflows SET project_id=NULL WHERE resource_kinds='["task"]'::jsonb;
ALTER TABLE workflow_versions ENABLE TRIGGER workflow_version_immutable;
ALTER TABLE workflows ADD CONSTRAINT developer_workflow_task_only CHECK(project_id IS NOT NULL OR resource_kinds='["task"]'::jsonb);
ALTER TABLE workflow_versions ADD CONSTRAINT developer_workflow_version_task_only CHECK(project_id IS NOT NULL OR resource_kinds='["task"]'::jsonb);
CREATE TABLE developer_workflows (
 actor_id text PRIMARY KEY REFERENCES actors(id),
 workflow_id text NOT NULL REFERENCES workflows(id),
 workflow_version_id text NOT NULL,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 assigned_by text NOT NULL REFERENCES actors(id),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(workflow_version_id,workflow_id) REFERENCES workflow_versions(id,workflow_id)
);
CREATE INDEX developer_workflows_members ON developer_workflows(workflow_id,actor_id);
-- Do not guess which workflow/state to keep if legacy active tasks disagree.
DO $$ BEGIN
 IF EXISTS (
  SELECT t.assignee_id FROM tasks t JOIN workflows w ON w.id=t.workflow_id
  WHERE t.assignee_id IS NOT NULL AND NOT t.archived
  GROUP BY t.assignee_id HAVING count(DISTINCT t.workflow_id)>1 OR bool_or(w.project_id IS NOT NULL)
 ) THEN RAISE EXCEPTION 'Legacy developer tasks need an explicit workflow/state reconciliation before migration'; END IF;
END $$;
INSERT INTO developer_workflows(actor_id,workflow_id,workflow_version_id,assigned_by)
 SELECT t.assignee_id,w.id,w.latest_version_id,min(t.created_by)
 FROM tasks t JOIN workflows w ON w.id=t.workflow_id
 WHERE t.assignee_id IS NOT NULL AND NOT t.archived AND w.project_id IS NULL AND w.latest_version_id IS NOT NULL
 GROUP BY t.assignee_id,w.id,w.latest_version_id;
CREATE OR REPLACE FUNCTION validate_task_workflow_state() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM workflow_versions v WHERE v.id=NEW.workflow_version_id AND v.workflow_id=NEW.workflow_id AND (v.project_id IS NULL OR v.project_id=NEW.project_id) AND v.resource_kinds @> '["task"]'::jsonb AND EXISTS(SELECT 1 FROM jsonb_array_elements(v.graph->'states') s WHERE s->>'id'=NEW.state_id)) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='task state outside pinned workflow';
 END IF;
 RETURN NEW;
END $$;
CREATE FUNCTION enforce_developer_task_workflow() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT NEW.archived AND EXISTS(SELECT 1 FROM workflows WHERE id=NEW.workflow_id AND project_id IS NULL) THEN
  PERFORM id FROM actors WHERE id=NEW.assignee_id FOR UPDATE;
  IF NOT EXISTS(SELECT 1 FROM developer_workflows d WHERE d.actor_id=NEW.assignee_id AND d.workflow_id=NEW.workflow_id) THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='task must use its assigned developer workflow';
  END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER developer_task_workflow BEFORE INSERT OR UPDATE ON tasks FOR EACH ROW EXECUTE FUNCTION enforce_developer_task_workflow();
CREATE FUNCTION enforce_developer_workflow_membership() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE developer text; BEGIN
 developer=CASE WHEN TG_OP='DELETE' THEN OLD.actor_id ELSE NEW.actor_id END;
 PERFORM id FROM actors WHERE id=developer FOR UPDATE;
 IF TG_OP<>'DELETE' AND NOT EXISTS(SELECT 1 FROM workflow_versions v JOIN workflows w ON w.id=v.workflow_id WHERE v.id=NEW.workflow_version_id AND w.id=NEW.workflow_id AND w.project_id IS NULL AND NOT w.archived) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='developer requires a published global task workflow';
 END IF;
 IF EXISTS(SELECT 1 FROM tasks t JOIN workflows w ON w.id=t.workflow_id WHERE t.assignee_id=developer AND NOT t.archived AND w.project_id IS NULL AND (TG_OP='DELETE' OR t.workflow_id<>NEW.workflow_id)) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='reassign existing tasks before changing developer workflow';
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
END $$;
CREATE TRIGGER developer_workflow_membership BEFORE INSERT OR UPDATE OR DELETE ON developer_workflows FOR EACH ROW EXECUTE FUNCTION enforce_developer_workflow_membership();
-- Legacy document policies retain project integrity; global task policies use null scope.
ALTER TABLE workflow_versions ADD CONSTRAINT workflow_version_project_scope_fk FOREIGN KEY(workflow_id,project_id) REFERENCES workflows(id,project_id);
CREATE FUNCTION validate_workflow_version_scope() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM workflows w WHERE w.id=NEW.workflow_id AND w.project_id IS NOT DISTINCT FROM NEW.project_id) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='published workflow scope differs from its definition';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER workflow_version_scope BEFORE INSERT ON workflow_versions FOR EACH ROW EXECUTE FUNCTION validate_workflow_version_scope();
CREATE FUNCTION validate_backlog_workflow_scope() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM workflow_versions v WHERE v.id=NEW.workflow_version_id AND v.workflow_id=NEW.workflow_id AND (v.project_id IS NULL OR v.project_id=NEW.project_id) AND v.resource_kinds @> '["task"]'::jsonb) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='backlog default workflow has an incompatible scope';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER backlog_workflow_scope BEFORE INSERT OR UPDATE ON backlogs FOR EACH ROW EXECUTE FUNCTION validate_backlog_workflow_scope();
