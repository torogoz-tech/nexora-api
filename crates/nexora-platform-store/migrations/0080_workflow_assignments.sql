INSERT INTO actor_grants(actor_id,capability,project_id)
SELECT r.actor_id,'workflow-assignments:manage',NULL FROM actor_roles r
WHERE r.role IN ('technical_lead','pmo','manager','super_admin')
AND NOT EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=r.actor_id AND g.capability='workflow-assignments:manage' AND g.project_id IS NULL);
