ALTER TABLE developer_workflows DROP CONSTRAINT developer_workflows_pkey;
ALTER TABLE developer_workflows ADD PRIMARY KEY(actor_id,workflow_id);

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
  IF EXISTS(SELECT 1 FROM tasks t WHERE t.assignee_id=OLD.actor_id AND t.workflow_id=OLD.workflow_id AND NOT t.archived AND t.lifecycle='active') THEN
   RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='reassign active tasks before removing workflow membership';
  END IF;
 END IF;
 IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
END $$;

CREATE FUNCTION enforce_member_workflow_role_limit() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM id FROM actors WHERE id=NEW.actor_id FOR UPDATE;
 IF NEW.role='developer' AND (SELECT count(*) FROM developer_workflows WHERE actor_id=NEW.actor_id)>1 THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='keep only one workflow before changing role to developer';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER member_workflow_role_limit BEFORE INSERT OR UPDATE ON actor_roles FOR EACH ROW EXECUTE FUNCTION enforce_member_workflow_role_limit();
