CREATE TABLE email_access_settings(singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),public_web_url text NOT NULL DEFAULT '',revision bigint NOT NULL DEFAULT 1 CHECK(revision>0));
INSERT INTO email_access_settings(singleton) VALUES(true);
CREATE TABLE member_invitations(
 id text PRIMARY KEY,email text NOT NULL,name text NOT NULL,invited_by text NOT NULL REFERENCES actors(id),
 status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','accepted','revoked')),
 digest bytea NOT NULL UNIQUE CHECK(octet_length(digest)=32),token_secret jsonb,revision bigint NOT NULL DEFAULT 1,
 public_web_url text NOT NULL,expires_at timestamptz NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),accepted_by text REFERENCES actors(id),accepted_at timestamptz,
 CHECK(revision>0 AND octet_length(email)<=320 AND octet_length(name)<=160)
);
CREATE UNIQUE INDEX member_invitations_pending_email ON member_invitations(lower(email)) WHERE status='pending';
CREATE TABLE password_reset_requests(
 id text PRIMARY KEY,actor_id text NOT NULL REFERENCES users(actor_id),email text NOT NULL,digest bytea NOT NULL UNIQUE CHECK(octet_length(digest)=32),
 token_secret jsonb,public_web_url text NOT NULL,expires_at timestamptz NOT NULL,used_at timestamptz,created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX password_reset_actor ON password_reset_requests(actor_id,created_at);
CREATE TABLE email_action_requests(id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,address_digest bytea NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE INDEX email_action_requests_recent ON email_action_requests(created_at);
ALTER TABLE email_notifications DROP CONSTRAINT email_notifications_kind_check;
ALTER TABLE email_notifications ADD CONSTRAINT email_notifications_kind_check CHECK(kind IN ('assignment','approval','test','invitation','invitation_accepted','password_reset','approval_result'));
CREATE FUNCTION enqueue_approval_result_email() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.status='pending' AND NEW.status IN ('approved','rejected') THEN
 INSERT INTO email_notifications(event_key,kind,project_id,resource_id,actor_id)
 SELECT 'approval_result:'||NEW.id||':'||NEW.revision,'approval_result',NEW.project_id,NEW.id,r.actor_id
 FROM notification_recipients r JOIN actors a ON a.id=r.actor_id AND a.active AND a.kind='human'
 WHERE r.actor_id=NEW.requested_by AND r.email<>'' AND r.approvals ON CONFLICT DO NOTHING;
 END IF;RETURN NEW;
END $$;
CREATE TRIGGER approval_result_email AFTER UPDATE OF status ON approval_requests FOR EACH ROW EXECUTE FUNCTION enqueue_approval_result_email();
