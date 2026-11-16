n.recipient_id=$1 AND (
 (n.kind IN ('assignment','task_comment','task_updated') AND EXISTS(
  SELECT 1 FROM tasks t WHERE t.id=n.resource_id AND t.project_id IS NOT DISTINCT FROM n.project_id AND NOT t.archived
  AND ((t.project_id IS NULL AND $4) OR (t.project_id=ANY($2) AND
   (t.project_id=ANY($3) OR t.assignee_id=$1 OR t.created_by=$1 OR EXISTS(
    SELECT 1 FROM task_history h WHERE h.task_id=t.id AND h.project_id=t.project_id AND h.actor_id=$1
    AND h.operation IN ('task.created','task.changed','task.reassigned_workflow')
    AND h.details->'after'->>'assignee_id' IS NOT NULL
    AND h.details->'before'->>'assignee_id' IS DISTINCT FROM h.details->'after'->>'assignee_id'))))))
 OR (n.kind IN ('approval','approval_result') AND EXISTS(
  SELECT 1 FROM approval_requests q JOIN projects p ON p.id=q.project_id AND NOT p.archived
  WHERE q.id=n.resource_id AND q.project_id=n.project_id
  AND (n.kind<>'approval' OR EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability='approvals:grant' AND (g.project_id IS NULL OR g.project_id=q.project_id)))
  AND ((q.target_kind='document_version' AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability IN ('documents:read','approvals:grant') AND (g.project_id IS NULL OR g.project_id=q.project_id)))
   OR (q.target_kind='task' AND q.project_id=ANY($2) AND EXISTS(
    SELECT 1 FROM tasks t WHERE t.id=q.resource_id AND t.project_id=q.project_id AND NOT t.archived
    AND (t.project_id=ANY($3) OR t.assignee_id=$1 OR t.created_by=$1 OR EXISTS(
     SELECT 1 FROM task_history h WHERE h.task_id=t.id AND h.project_id=t.project_id AND h.actor_id=$1
     AND h.operation IN ('task.created','task.changed','task.reassigned_workflow')
     AND h.details->'after'->>'assignee_id' IS NOT NULL
     AND h.details->'before'->>'assignee_id' IS DISTINCT FROM h.details->'after'->>'assignee_id')))))))
 OR (n.kind IN ('calendar_created','calendar_updated','calendar_cancelled','calendar_attendance')
  AND EXISTS(SELECT 1 FROM team_calendar_activities e WHERE e.id=n.resource_id))

 OR (n.kind IN ('knowledge_review','knowledge_result') AND EXISTS(
 SELECT 1 FROM knowledge_reviews r WHERE r.id=n.resource_id AND r.project_id IS NOT DISTINCT FROM n.project_id
 AND (r.project_id IS NULL OR r.project_id=ANY($2))
 AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability=CASE WHEN r.project_id IS NULL THEN 'organization-knowledge:read' ELSE 'documents:read' END AND (g.project_id IS NULL OR g.project_id=r.project_id))))
 OR (n.kind IN ('workflow_assignment','role_changed','security_changed'))
 OR (n.kind='access_request' AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability='actors:manage' AND g.project_id IS NULL))
 OR (n.kind IN ('feature_approval','feature_result') AND EXISTS(SELECT 1 FROM feature_proposals f JOIN projects p ON p.id=f.project_id AND NOT p.archived WHERE f.id=n.resource_id AND f.project_id=n.project_id)
  AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability='documents:read' AND (g.project_id IS NULL OR g.project_id=n.project_id))
  AND n.project_id=ANY($2)
  AND (n.kind<>'feature_approval' OR (EXISTS(SELECT 1 FROM actor_roles r WHERE r.actor_id=$1 AND r.role IN ('technical_lead','pmo','manager','super_admin') AND (r.all_projects OR EXISTS(SELECT 1 FROM member_role_projects mp WHERE mp.actor_id=$1 AND mp.project_id=n.project_id))) AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability='approvals:grant' AND (g.project_id IS NULL OR g.project_id=n.project_id)))))
 OR (n.kind IN ('repository_failed','review_completed','review_failed','review_stale') AND EXISTS(
  SELECT 1 FROM project_repositories r JOIN projects p ON p.id=r.project_id AND NOT p.archived JOIN git_connections c ON c.id=r.connection_id AND c.status='active' JOIN actors owner ON owner.id=c.owner_actor_id AND owner.active WHERE r.id=n.resource_id AND r.project_id=n.project_id AND NOT r.archived AND r.authorization_status='authorized'
  AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability='repositories:read' AND (g.project_id IS NULL OR g.project_id=n.project_id)) AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability='knowledge:read' AND (g.project_id IS NULL OR g.project_id=n.project_id))))
 OR (n.kind IN ('leave_request','leave_result') AND EXISTS(SELECT 1 FROM labor_leave_requests l WHERE l.id=n.resource_id AND (l.actor_id=$1 OR inbox_may_review_labor($1,l.actor_id,false))))
 OR (n.kind IN ('overtime_request','overtime_result') AND EXISTS(SELECT 1 FROM labor_overtime_requests l WHERE l.id=n.resource_id AND (l.actor_id=$1 OR inbox_may_review_labor($1,l.actor_id,true))))
 OR (n.kind IN ('holiday_request','holiday_result') AND EXISTS(SELECT 1 FROM labor_holidays h WHERE h.id=n.resource_id AND (h.proposed_by=$1 OR EXISTS(SELECT 1 FROM actor_roles r WHERE r.actor_id=$1 AND (r.role='super_admin' OR r.role IN ('pmo','manager') AND r.all_projects)))))
)
