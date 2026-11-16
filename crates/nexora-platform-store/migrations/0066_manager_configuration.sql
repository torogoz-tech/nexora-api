-- Explicit global configuration authority for existing managers. Operational scope is unchanged.
INSERT INTO actor_grants(actor_id,capability,project_id)
SELECT r.actor_id,c.capability,NULL
FROM actor_roles r CROSS JOIN (VALUES
 ('installation:manage'),('actors:manage'),('keys:manage'),('projects:read'),('projects:write'),
 ('repositories:read'),('repositories:write'),('repositories:sync'),('connections:manage'),
 ('workflows:read'),('workflows:write'),('workflows:publish')
) AS c(capability) WHERE r.role='manager'
ON CONFLICT DO NOTHING;
INSERT INTO audit_events(id,actor_id,operation,resource_id,metadata)
SELECT 'aud_'||md5('manager_configuration_66:'||actor_id),NULL,'member.configuration_authority_migrated',actor_id,
 jsonb_build_object('role','manager','scope','installation','migration',66)
FROM actor_roles WHERE role='manager';
