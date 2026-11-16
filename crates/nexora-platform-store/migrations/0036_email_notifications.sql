CREATE TABLE notification_recipients (
 actor_id text PRIMARY KEY REFERENCES actors(id), email text NOT NULL DEFAULT '',
 assignments boolean NOT NULL DEFAULT true, approvals boolean NOT NULL DEFAULT true,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0), CHECK(octet_length(email)<=320)
);
CREATE TABLE email_notifications (
 event_key text PRIMARY KEY, kind text NOT NULL CHECK(kind IN ('assignment','approval','test')),
 project_id text REFERENCES projects(id), recipient_override text, resource_id text NOT NULL,
 actor_id text NOT NULL REFERENCES actors(id),
 status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','delivering','sent','skipped','failed')),
 attempts integer NOT NULL DEFAULT 0, available_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(), delivered_at timestamptz
);
CREATE INDEX email_notifications_pending ON email_notifications(available_at) WHERE status IN ('pending','delivering');
CREATE FUNCTION enqueue_assignment_email() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.assignee_id IS NOT NULL AND NOT NEW.archived AND (TG_OP='INSERT' OR NEW.assignee_id IS DISTINCT FROM OLD.assignee_id) THEN
  INSERT INTO email_notifications(event_key,kind,project_id,resource_id,actor_id)
  SELECT 'assignment:'||NEW.id||':'||NEW.revision||':'||NEW.assignee_id,'assignment',NEW.project_id,NEW.id,NEW.assignee_id
  FROM notification_recipients WHERE actor_id=NEW.assignee_id AND email<>'' AND assignments ON CONFLICT DO NOTHING;
 END IF;RETURN NEW;
END $$;
CREATE TRIGGER task_assignment_email AFTER INSERT OR UPDATE OF assignee_id ON tasks FOR EACH ROW EXECUTE FUNCTION enqueue_assignment_email();
CREATE FUNCTION enqueue_approval_email() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO email_notifications(event_key,kind,project_id,resource_id,actor_id)
 SELECT 'approval:'||NEW.id||':'||r.actor_id,'approval',NEW.project_id,NEW.id,r.actor_id
 FROM notification_recipients r JOIN actors a ON a.id=r.actor_id AND a.active AND a.kind='human'
 WHERE r.email<>'' AND r.approvals AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=r.actor_id AND g.capability='approvals:grant' AND (g.project_id IS NULL OR g.project_id=NEW.project_id))
 ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;
CREATE TRIGGER approval_request_email AFTER INSERT ON approval_requests FOR EACH ROW EXECUTE FUNCTION enqueue_approval_email();
