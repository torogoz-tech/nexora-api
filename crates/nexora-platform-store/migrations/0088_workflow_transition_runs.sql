DO $$ DECLARE dependency record; BEGIN
 FOR dependency IN SELECT conname FROM pg_constraint WHERE contype='f' AND conrelid='task_approvals'::regclass AND confrelid='workflow_versions'::regclass
 LOOP EXECUTE format('ALTER TABLE task_approvals DROP CONSTRAINT %I',dependency.conname); END LOOP;
END $$;
ALTER TABLE task_approvals ADD CONSTRAINT approval_workflow_pair_fk FOREIGN KEY(workflow_version_id,workflow_id) REFERENCES workflow_versions(id,workflow_id);
-- Execution workflows may be installation-wide; operation scope remains checked by the application service.
ALTER TABLE approval_requests DROP CONSTRAINT approval_requests_workflow_version_id_project_id_fkey;
ALTER TABLE approval_requests ADD CONSTRAINT approval_requests_workflow_version_id_fkey FOREIGN KEY(workflow_version_id) REFERENCES workflow_versions(id);
CREATE TABLE task_transition_runs (
 id text PRIMARY KEY,
 task_id text NOT NULL REFERENCES tasks(id),
 project_id text REFERENCES projects(id),
 actor_id text NOT NULL REFERENCES actors(id),
 credential_id text NOT NULL,
 auth_method text NOT NULL CHECK(auth_method IN ('session','api_key')),
 task_revision bigint NOT NULL,
 transition_id text NOT NULL,
 fingerprint text NOT NULL,
 parameters jsonb NOT NULL,
 record jsonb NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(task_id,task_revision,transition_id,actor_id,fingerprint)
);
CREATE INDEX task_transition_runs_task ON task_transition_runs(task_id,created_at DESC,id DESC);
ALTER TABLE email_notifications DROP CONSTRAINT email_notifications_kind_check;
ALTER TABLE email_notifications ADD CONSTRAINT email_notifications_kind_check CHECK(kind IN ('assignment','approval','test','invitation','invitation_accepted','password_reset','approval_result','email_verification','email_changed','workflow_transition'));
ALTER TABLE installation_email_contents DROP CONSTRAINT installation_email_contents_kind_check;
ALTER TABLE installation_email_contents ADD CONSTRAINT installation_email_contents_kind_check CHECK(kind IN ('assignment','approval','test','invitation','invitation_accepted','password_reset','approval_result','email_verification','email_changed','workflow_transition'));
INSERT INTO installation_email_contents(kind,language,subject,body_html)
SELECT 'workflow_transition',language,CASE WHEN language='es' THEN 'Tarea actualizada en {{organization_name}}' ELSE 'Task updated in {{organization_name}}' END,
 CASE WHEN language='es' THEN '<p>Hola {{recipient_name}},</p><p>La tarea {{resource_id}} cambió de estado.</p>' ELSE '<p>Hello {{recipient_name}},</p><p>Task {{resource_id}} changed state.</p>' END
FROM installation_email_contents WHERE kind='assignment';
-- Delivery is asynchronous. Surface terminal SMTP failures without undoing state advancement.
CREATE FUNCTION fail_workflow_notification_run() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE run task_transition_runs; task tasks; next_sequence bigint; step integer;
BEGIN
 IF NEW.kind<>'workflow_transition' OR NEW.status<>'failed' OR NEW.status IS NOT DISTINCT FROM OLD.status THEN RETURN NEW; END IF;
 SELECT * INTO run FROM task_transition_runs WHERE split_part(NEW.event_key,':',1)='workflow' AND split_part(NEW.event_key,':',2)=id FOR UPDATE;
 IF NOT FOUND THEN RETURN NEW; END IF;
 step:=split_part(NEW.event_key,':',3)::integer;
 UPDATE task_transition_runs SET record=record||jsonb_build_object('status','failed','next_step',LEAST(COALESCE((record->>'next_step')::integer,step),step),'error_code','notification_delivery_failed','message','Email delivery failed after retries. State has already advanced. Retry this run to requeue the notification.'),updated_at=clock_timestamp() WHERE id=run.id;
 UPDATE tasks SET history_sequence=history_sequence+1 WHERE id=run.task_id RETURNING * INTO task;
 INSERT INTO task_history(id,project_id,task_id,sequence,revision,operation,actor_id,credential_id,details)
 VALUES('smtp:'||NEW.event_key||':'||task.history_sequence,task.project_id,task.id,task.history_sequence,task.revision,'task.transition_failed',run.actor_id,run.credential_id,jsonb_build_object('run_id',run.id,'transition_id',run.transition_id,'step_index',step,'code','notification_delivery_failed','message','Email delivery failed after retries. State has already advanced. Retry this run to requeue the notification.'));
 RETURN NEW;
END $$;
CREATE TRIGGER workflow_notification_failure AFTER UPDATE OF status ON email_notifications FOR EACH ROW EXECUTE FUNCTION fail_workflow_notification_run();
