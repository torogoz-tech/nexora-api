to_jsonb(n)-'recipient_id' || jsonb_build_object(
 'resource_title', CASE
 WHEN n.kind IN ('assignment','task_comment','task_updated') THEN (SELECT title FROM tasks WHERE id=n.resource_id)
 WHEN n.kind IN ('approval','approval_result') THEN (SELECT CASE WHEN q.target_kind='task' THEN (SELECT title FROM tasks WHERE id=q.resource_id) ELSE (SELECT title FROM project_document_versions WHERE id=q.version_id) END FROM approval_requests q WHERE q.id=n.resource_id)
 WHEN n.kind LIKE 'calendar_%' THEN (SELECT title FROM team_calendar_activities WHERE id=n.resource_id)
 WHEN n.kind IN ('knowledge_review','knowledge_result') THEN (SELECT title FROM knowledge_reviews WHERE id=n.resource_id)
 WHEN n.kind LIKE 'feature_%' THEN (SELECT requirement_title FROM feature_proposals WHERE id=n.resource_id)
 WHEN n.kind='repository_failed' OR n.kind LIKE 'review_%' THEN (SELECT COALESCE(remote_full_name,requested_full_name) FROM project_repositories WHERE id=n.resource_id)
 WHEN n.kind='workflow_assignment' THEN (SELECT name FROM workflows WHERE id=n.resource_id)
 WHEN n.kind LIKE 'holiday_%' THEN (SELECT name FROM labor_holidays WHERE id=n.resource_id)
 ELSE NULL END,
 'sender_name', CASE WHEN n.kind='task_comment' THEN (SELECT a.name FROM task_comments c JOIN actors a ON a.id=c.actor_id WHERE c.id=split_part(n.id,':',2))
 WHEN n.kind='calendar_attendance' THEN (SELECT name FROM actors WHERE id=split_part(n.id,':',3))
 WHEN n.kind LIKE 'leave_%' THEN (SELECT a.name FROM labor_leave_requests l JOIN actors a ON a.id=l.actor_id WHERE l.id=n.resource_id)
 WHEN n.kind LIKE 'overtime_%' THEN (SELECT a.name FROM labor_overtime_requests l JOIN actors a ON a.id=l.actor_id WHERE l.id=n.resource_id)
 WHEN n.kind='access_request' THEN (SELECT name FROM actors WHERE id=n.resource_id)
 ELSE NULL END,
 'event_date',CASE WHEN n.kind LIKE 'calendar_%' THEN (SELECT start_date::text FROM team_calendar_activities WHERE id=n.resource_id)
 WHEN n.kind LIKE 'leave_%' THEN (SELECT start_date::text FROM labor_leave_requests WHERE id=n.resource_id)
 WHEN n.kind LIKE 'overtime_%' THEN (SELECT date::text FROM labor_overtime_requests WHERE id=n.resource_id)
 WHEN n.kind LIKE 'holiday_%' THEN (SELECT date::text FROM labor_holidays WHERE id=n.resource_id) ELSE NULL END,
 'decision',CASE WHEN n.kind='approval_result' THEN (SELECT status FROM approval_requests WHERE id=n.resource_id)
 WHEN n.kind='feature_result' THEN (SELECT status FROM feature_proposals WHERE id=n.resource_id)
 WHEN n.kind='leave_result' THEN (SELECT status FROM labor_leave_requests WHERE id=n.resource_id)
 WHEN n.kind='overtime_result' THEN (SELECT status FROM labor_overtime_requests WHERE id=n.resource_id)
 WHEN n.kind='holiday_result' THEN (SELECT status FROM labor_holidays WHERE id=n.resource_id) ELSE NULL END)
