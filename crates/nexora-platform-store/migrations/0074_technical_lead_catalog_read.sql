INSERT INTO actor_grants(actor_id,capability,project_id)
 SELECT r.actor_id,c.capability,NULL FROM actor_roles r
 CROSS JOIN (VALUES ('members:read'),('connections:read'),('task-types:read'),('analysis-profiles:read'),('repositories:read')) c(capability)
 WHERE r.role='technical_lead'
 AND NOT EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=r.actor_id AND g.capability=c.capability AND g.project_id IS NULL);

INSERT INTO actor_grants(actor_id,capability,project_id)
 SELECT g.actor_id,'member-projects:manage',g.project_id FROM actor_grants g JOIN actor_roles r ON r.actor_id=g.actor_id
 WHERE r.role='technical_lead' AND g.capability='projects:read';
