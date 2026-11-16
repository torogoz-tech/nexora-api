-- Durable personal inbox. No email address, credential, reason or comment body is copied here.
CREATE TABLE in_app_notifications (
 id text PRIMARY KEY,
 recipient_id text NOT NULL REFERENCES users(actor_id),
 kind text NOT NULL,
 project_id text REFERENCES projects(id),
 resource_id text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 read_at timestamptz,
 archived_at timestamptz
);
CREATE INDEX in_app_notifications_recipient_page ON in_app_notifications(recipient_id,created_at DESC,id DESC);
CREATE INDEX in_app_notifications_unread ON in_app_notifications(recipient_id) WHERE read_at IS NULL AND archived_at IS NULL;
CREATE FUNCTION notify_task_assignment() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.assignee_id IS NOT NULL AND NOT NEW.archived AND (TG_OP='INSERT' OR NEW.assignee_id IS DISTINCT FROM OLD.assignee_id) THEN
 INSERT INTO in_app_notifications(id,recipient_id,kind,project_id,resource_id)
 SELECT 'assignment:'||NEW.id||':'||NEW.revision||':'||NEW.assignee_id,NEW.assignee_id,'assignment',NEW.project_id,NEW.id
 FROM users u JOIN actors a ON a.id=u.actor_id WHERE u.actor_id=NEW.assignee_id AND a.active
 ON CONFLICT DO NOTHING;
 END IF; RETURN NEW;
END $$;
CREATE TRIGGER inbox_task_assignment AFTER INSERT OR UPDATE OF assignee_id ON tasks FOR EACH ROW EXECUTE FUNCTION notify_task_assignment();
CREATE FUNCTION notify_task_comment() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO in_app_notifications(id,recipient_id,kind,project_id,resource_id)
 SELECT 'comment:'||NEW.id||':'||u.actor_id,u.actor_id,'task_comment',NEW.project_id,NEW.task_id
 FROM users u JOIN actors a ON a.id=u.actor_id AND a.active JOIN tasks t ON t.id=NEW.task_id
 WHERE u.actor_id<>NEW.actor_id AND NOT t.archived AND
 (u.actor_id=t.assignee_id OR u.actor_id=t.created_by OR EXISTS(SELECT 1 FROM task_comments c WHERE c.task_id=t.id AND c.actor_id=u.actor_id AND c.id<>NEW.id))
 ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;
CREATE TRIGGER inbox_task_comment AFTER INSERT ON task_comments FOR EACH ROW EXECUTE FUNCTION notify_task_comment();
CREATE FUNCTION notify_approval_request() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='INSERT' THEN
 INSERT INTO in_app_notifications(id,recipient_id,kind,project_id,resource_id)
 SELECT 'approval:'||NEW.id||':'||u.actor_id,u.actor_id,'approval',NEW.project_id,NEW.id
 FROM users u JOIN actors a ON a.id=u.actor_id AND a.active
 WHERE u.actor_id<>NEW.requested_by AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=u.actor_id AND g.capability='approvals:grant' AND (g.project_id IS NULL OR g.project_id=NEW.project_id))
 ON CONFLICT DO NOTHING;
 ELSIF NEW.status IS DISTINCT FROM OLD.status THEN
 INSERT INTO in_app_notifications(id,recipient_id,kind,project_id,resource_id)
 SELECT 'approval_result:'||NEW.id||':'||NEW.revision,NEW.requested_by,'approval_result',NEW.project_id,NEW.id
 FROM users u JOIN actors a ON a.id=u.actor_id AND a.active WHERE u.actor_id=NEW.requested_by
 ON CONFLICT DO NOTHING;
 END IF; RETURN NEW;
END $$;
CREATE TRIGGER inbox_approval_request AFTER INSERT OR UPDATE OF status ON approval_requests FOR EACH ROW EXECUTE FUNCTION notify_approval_request();
CREATE FUNCTION notify_calendar_activity() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE event_kind text; event_revision bigint;
BEGIN
 IF NEW.operation NOT IN ('calendar.activity.create','calendar.activity.change','calendar.activity.cancel') THEN RETURN NEW; END IF;
 SELECT revision INTO event_revision FROM team_calendar_activities WHERE id=NEW.resource_id;
 event_kind:=CASE NEW.operation WHEN 'calendar.activity.create' THEN 'calendar_created' WHEN 'calendar.activity.cancel' THEN 'calendar_cancelled' ELSE 'calendar_updated' END;
 INSERT INTO in_app_notifications(id,recipient_id,kind,resource_id)
 SELECT event_kind||':'||NEW.resource_id||':'||event_revision||':'||u.actor_id,u.actor_id,event_kind,NEW.resource_id
 FROM users u JOIN actors a ON a.id=u.actor_id AND a.active
 WHERE u.actor_id IS DISTINCT FROM NEW.actor_id ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;
CREATE TRIGGER inbox_calendar_activity AFTER INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION notify_calendar_activity();
CREATE FUNCTION notify_calendar_attendance() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' AND NEW.response=OLD.response AND NEW.activity_revision=OLD.activity_revision THEN RETURN NEW; END IF;
 INSERT INTO in_app_notifications(id,recipient_id,kind,resource_id)
 SELECT 'attendance:'||NEW.activity_id||':'||NEW.actor_id||':'||NEW.updated_at::text,e.created_by,'calendar_attendance',e.id
 FROM team_calendar_activities e JOIN users u ON u.actor_id=e.created_by JOIN actors a ON a.id=u.actor_id AND a.active
 WHERE e.id=NEW.activity_id AND e.created_by<>NEW.actor_id ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;
CREATE TRIGGER inbox_calendar_attendance AFTER INSERT OR UPDATE ON calendar_activity_attendance FOR EACH ROW EXECUTE FUNCTION notify_calendar_attendance();
-- Actor-scoped invalidations reveal neither another member's activity nor message counts.
CREATE FUNCTION signal_personal_inbox_update() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO live_update_revisions(scope_id,topic) VALUES('actor:'||NEW.recipient_id,'notifications')
 ON CONFLICT(scope_id,topic) DO UPDATE SET revision=live_update_revisions.revision+1;
 RETURN NULL;
END $$;
CREATE TRIGGER inbox_live_update AFTER INSERT OR UPDATE ON in_app_notifications FOR EACH ROW EXECUTE FUNCTION signal_personal_inbox_update();

-- Same current approver rules as LaborCalendarPolicy / overtime_approval_authority.
CREATE FUNCTION inbox_may_review_labor(reviewer text, target text, overtime boolean) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT reviewer<>target AND EXISTS(SELECT 1 FROM actors WHERE id=target AND active AND kind='human') AND (
 CASE WHEN overtime AND NOT EXISTS(SELECT 1 FROM actor_roles WHERE actor_id=target AND role IN ('developer','technical_lead'))
 THEN EXISTS(SELECT 1 FROM actor_roles WHERE actor_id=reviewer AND role IN ('manager','super_admin'))
 ELSE EXISTS(SELECT 1 FROM actor_grants WHERE actor_id=reviewer AND capability='installation:manage' AND project_id IS NULL)
 OR CASE WHEN EXISTS(SELECT 1 FROM labor_approvers WHERE actor_id=target AND approver_id IS NOT NULL)
 THEN EXISTS(SELECT 1 FROM labor_approvers l JOIN actor_roles r ON r.actor_id=l.approver_id WHERE l.actor_id=target AND l.approver_id=reviewer AND r.role IN ('pmo','manager','technical_lead') AND (r.all_projects OR
  (EXISTS(SELECT 1 FROM member_role_projects WHERE actor_id=target) AND NOT EXISTS(SELECT 1 FROM actor_roles WHERE actor_id=target AND all_projects)
   AND NOT EXISTS(SELECT 1 FROM member_role_projects p WHERE p.actor_id=target AND NOT EXISTS(SELECT 1 FROM member_role_projects q WHERE q.actor_id=reviewer AND q.project_id=p.project_id)))))
 ELSE EXISTS(SELECT 1 FROM actor_roles WHERE actor_id=reviewer AND role IN ('manager','technical_lead')) END END)
$$;
CREATE FUNCTION notify_labor_request() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE v jsonb:=to_jsonb(NEW); event_kind text; target text;
BEGIN
 target:=COALESCE(v->>'actor_id',v->>'proposed_by');
 event_kind:=TG_ARGV[0]||CASE WHEN TG_OP='INSERT' THEN '_request' ELSE '_result' END;
 IF TG_OP='UPDATE' AND NEW.status IS NOT DISTINCT FROM OLD.status THEN RETURN NEW; END IF;
 INSERT INTO in_app_notifications(id,recipient_id,kind,resource_id)
 SELECT event_kind||':'||NEW.id||':'||u.actor_id,u.actor_id,event_kind,NEW.id FROM users u JOIN actors a ON a.id=u.actor_id AND a.active
 WHERE CASE WHEN TG_OP='UPDATE' THEN u.actor_id=target
 WHEN TG_ARGV[0]='holiday' THEN u.actor_id<>target AND EXISTS(SELECT 1 FROM actor_roles r WHERE r.actor_id=u.actor_id AND (r.role='super_admin' OR r.role IN ('pmo','manager') AND r.all_projects))
 ELSE inbox_may_review_labor(u.actor_id,target,TG_ARGV[0]='overtime') END
 ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;
CREATE TRIGGER inbox_leave AFTER INSERT OR UPDATE OF status ON labor_leave_requests FOR EACH ROW EXECUTE FUNCTION notify_labor_request('leave');
CREATE TRIGGER inbox_overtime AFTER INSERT OR UPDATE OF status ON labor_overtime_requests FOR EACH ROW EXECUTE FUNCTION notify_labor_request('overtime');
CREATE TRIGGER inbox_holiday AFTER INSERT OR UPDATE OF status ON labor_holidays FOR EACH ROW EXECUTE FUNCTION notify_labor_request('holiday');
CREATE FUNCTION notify_feature_proposal() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE event_kind text;
BEGIN
 IF TG_OP='UPDATE' AND NEW.status IS NOT DISTINCT FROM OLD.status THEN RETURN NEW; END IF;
 event_kind:=CASE WHEN TG_OP='INSERT' THEN 'feature_approval' ELSE 'feature_result' END;
 INSERT INTO in_app_notifications(id,recipient_id,kind,project_id,resource_id)
 SELECT event_kind||':'||NEW.id||':'||u.actor_id,u.actor_id,event_kind,NEW.project_id,NEW.id FROM users u JOIN actors a ON a.id=u.actor_id AND a.active
 WHERE CASE WHEN TG_OP='UPDATE' THEN u.actor_id=NEW.proposed_by ELSE u.actor_id<>NEW.proposed_by AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=u.actor_id AND g.capability='approvals:grant' AND (g.project_id IS NULL OR g.project_id=NEW.project_id)) END
 ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;
CREATE TRIGGER inbox_feature_proposal AFTER INSERT OR UPDATE OF status ON feature_proposals FOR EACH ROW EXECUTE FUNCTION notify_feature_proposal();
CREATE FUNCTION notify_member_change() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE v jsonb; event_kind text; target text; resource text;
BEGIN
 v:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
 target:=v->>'actor_id'; event_kind:=TG_ARGV[0]; resource:=COALESCE(v->>'workflow_id',target);
 IF TG_OP='UPDATE' AND TG_TABLE_NAME='developer_workflows' AND v->>'workflow_version_id'=to_jsonb(OLD)->>'workflow_version_id' THEN RETURN NEW; END IF;
 INSERT INTO in_app_notifications(id,recipient_id,kind,resource_id)
 SELECT event_kind||':'||target||':'||resource||':'||COALESCE(v->>'revision','0')||':'||TG_OP,target,event_kind,resource FROM users u JOIN actors a ON a.id=u.actor_id AND a.active WHERE u.actor_id=target
 ON CONFLICT DO NOTHING;
 IF TG_OP='DELETE' THEN RETURN OLD; ELSE RETURN NEW; END IF;
END $$;
CREATE TRIGGER inbox_workflow_assignment AFTER INSERT OR UPDATE OR DELETE ON developer_workflows FOR EACH ROW EXECUTE FUNCTION notify_member_change('workflow_assignment');
CREATE TRIGGER inbox_member_role AFTER INSERT OR UPDATE ON actor_roles FOR EACH ROW EXECUTE FUNCTION notify_member_change('role_changed');
CREATE FUNCTION notify_repository_result() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE v jsonb:=to_jsonb(NEW); event_kind text;
BEGIN
 IF TG_TABLE_NAME='repository_jobs' THEN
 IF NEW.state<>'failed' OR NEW.state IS NOT DISTINCT FROM OLD.state THEN RETURN NEW; END IF;
 event_kind:='repository_failed';
 ELSE
 IF NEW.status NOT IN ('completed','failed','stale') OR NEW.status IS NOT DISTINCT FROM OLD.status THEN RETURN NEW; END IF;
 event_kind:='review_'||NEW.status;
 END IF;
 INSERT INTO in_app_notifications(id,recipient_id,kind,project_id,resource_id)
 SELECT event_kind||':'||NEW.id,NEW.requested_by,event_kind,NEW.project_id,NEW.repository_id FROM users u JOIN actors a ON a.id=u.actor_id AND a.active WHERE u.actor_id=NEW.requested_by
 ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;
CREATE TRIGGER inbox_repository_failure AFTER UPDATE ON repository_jobs FOR EACH ROW EXECUTE FUNCTION notify_repository_result();
CREATE TRIGGER inbox_review_result AFTER UPDATE ON pull_request_reviews FOR EACH ROW EXECUTE FUNCTION notify_repository_result();

CREATE FUNCTION notify_identity_audit() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.operation='identity.access_requested' THEN
 INSERT INTO in_app_notifications(id,recipient_id,kind,resource_id)
 SELECT 'access_request:'||NEW.id||':'||u.actor_id,u.actor_id,'access_request',NEW.resource_id FROM users u JOIN actors a ON a.id=u.actor_id AND a.active
 WHERE EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=u.actor_id AND g.capability='actors:manage' AND g.project_id IS NULL) ON CONFLICT DO NOTHING;
 ELSIF NEW.operation IN ('identity.totp_exemption','user.email.verified','member.restored') THEN
 INSERT INTO in_app_notifications(id,recipient_id,kind,resource_id)
 SELECT 'security:'||NEW.id,u.actor_id,'security_changed',u.actor_id FROM users u JOIN actors a ON a.id=u.actor_id AND a.active WHERE u.actor_id=CASE WHEN NEW.operation='user.email.verified' THEN NEW.actor_id ELSE NEW.resource_id END ON CONFLICT DO NOTHING;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER inbox_identity_security AFTER INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION notify_identity_audit();
CREATE FUNCTION notify_task_progress() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.operation NOT IN ('task.transitioned','task.taken','task.dependency_added','task.dependency_removed','task.dependency_resolved','task.subtask_added','task.subtask_removed','task.project_changed') AND NOT (
  NEW.operation='task.changed' AND (NEW.details->'before'->>'blocked') IS DISTINCT FROM (NEW.details->'after'->>'blocked')) THEN RETURN NEW; END IF;
 INSERT INTO in_app_notifications(id,recipient_id,kind,project_id,resource_id)
 SELECT 'progress:'||NEW.id||':'||u.actor_id,u.actor_id,'task_updated',NEW.project_id,NEW.task_id FROM users u JOIN actors a ON a.id=u.actor_id AND a.active JOIN tasks t ON t.id=NEW.task_id
 WHERE NOT t.archived AND u.actor_id<>NEW.actor_id AND
 (u.actor_id=t.assignee_id OR u.actor_id=t.created_by OR EXISTS(SELECT 1 FROM task_comments c WHERE c.task_id=t.id AND c.actor_id=u.actor_id))
 ON CONFLICT DO NOTHING;
 RETURN NEW;
END $$;
CREATE TRIGGER inbox_task_progress AFTER INSERT ON task_history FOR EACH ROW EXECUTE FUNCTION notify_task_progress();
CREATE FUNCTION signal_inbox_permission_update() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE target text;
BEGIN
 target:=COALESCE(to_jsonb(NEW)->>'actor_id',to_jsonb(OLD)->>'actor_id');
 INSERT INTO live_update_revisions(scope_id,topic) VALUES('actor:'||target,'notifications')
 ON CONFLICT(scope_id,topic) DO UPDATE SET revision=live_update_revisions.revision+1;
 RETURN NULL;
END $$;
CREATE TRIGGER inbox_grant_invalidation AFTER INSERT OR UPDATE OR DELETE ON actor_grants FOR EACH ROW EXECUTE FUNCTION signal_inbox_permission_update();
CREATE TRIGGER inbox_role_scope_invalidation AFTER INSERT OR UPDATE OR DELETE ON member_role_projects FOR EACH ROW EXECUTE FUNCTION signal_inbox_permission_update();
CREATE FUNCTION signal_task_inbox_access_update() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO live_update_revisions(scope_id,topic)
 SELECT DISTINCT 'actor:'||recipient_id,'notifications' FROM in_app_notifications n
 WHERE (n.resource_id=NEW.id AND n.kind IN ('assignment','task_comment','task_updated'))
 OR EXISTS(SELECT 1 FROM approval_requests q WHERE q.id=n.resource_id AND q.target_kind='task' AND q.resource_id=NEW.id)
 ON CONFLICT(scope_id,topic) DO UPDATE SET revision=live_update_revisions.revision+1;
 RETURN NULL;
END $$;
CREATE TRIGGER inbox_task_access_invalidation AFTER UPDATE OF assignee_id,project_id,archived ON tasks FOR EACH ROW EXECUTE FUNCTION signal_task_inbox_access_update();
CREATE FUNCTION signal_resource_inbox_access_update() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO live_update_revisions(scope_id,topic)
 SELECT DISTINCT 'actor:'||recipient_id,'notifications' FROM in_app_notifications n
 WHERE (TG_TABLE_NAME='projects' AND n.project_id=NEW.id)
 OR (TG_TABLE_NAME='project_repositories' AND n.resource_id=NEW.id)
 OR (TG_TABLE_NAME='git_connections' AND EXISTS(SELECT 1 FROM project_repositories r WHERE r.id=n.resource_id AND r.connection_id=NEW.id))
 OR (TG_TABLE_NAME='actors' AND EXISTS(SELECT 1 FROM project_repositories r JOIN git_connections c ON c.id=r.connection_id WHERE r.id=n.resource_id AND c.owner_actor_id=NEW.id))
 ON CONFLICT(scope_id,topic) DO UPDATE SET revision=live_update_revisions.revision+1;
 RETURN NULL;
END $$;
CREATE TRIGGER inbox_project_access_invalidation AFTER UPDATE OF archived ON projects FOR EACH ROW EXECUTE FUNCTION signal_resource_inbox_access_update();
CREATE TRIGGER inbox_repository_access_invalidation AFTER UPDATE OF archived,authorization_status ON project_repositories FOR EACH ROW EXECUTE FUNCTION signal_resource_inbox_access_update();

CREATE TRIGGER inbox_connection_access_invalidation AFTER UPDATE OF status ON git_connections FOR EACH ROW EXECUTE FUNCTION signal_resource_inbox_access_update();
CREATE TRIGGER inbox_connection_owner_invalidation AFTER UPDATE OF active ON actors FOR EACH ROW EXECUTE FUNCTION signal_resource_inbox_access_update();
