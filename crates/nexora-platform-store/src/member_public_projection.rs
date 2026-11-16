/// Public identity fields shared by the directory and individual profiles.
pub(crate) fn member_public_projection() -> &'static str {
    r#"jsonb_build_object(
                'avatar_revision',COALESCE((SELECT revision FROM user_avatars WHERE actor_id=a.id),0),
                'archived',u.archived_at IS NOT NULL,'lifecycle_revision',u.lifecycle_revision,
                'id',a.id,'name',a.name,'username',u.username,'active',a.active,
                'email',COALESCE(r.email,''),'created_at',a.created_at,
                'last_login_at',(SELECT max(s.created_at) FROM sessions s WHERE s.actor_id=a.id AND s.access_stage='ready'),
                'role',(SELECT role FROM actor_roles WHERE actor_id=a.id),
                'role_all_projects',COALESCE((SELECT all_projects FROM actor_roles WHERE actor_id=a.id),false),
                'role_project_ids',COALESCE((SELECT jsonb_agg(project_id ORDER BY project_id) FROM member_role_projects WHERE actor_id=a.id),'[]'::jsonb),
                'position',COALESCE((SELECT position FROM member_positions WHERE actor_id=a.id),''),
                'position_revision',COALESCE((SELECT revision FROM member_positions WHERE actor_id=a.id),0),
                'role_revision',COALESCE((SELECT revision FROM actor_roles WHERE actor_id=a.id),0),
                'has_password',u.password_hash IS NOT NULL,
                'local_login_enabled',(SELECT local_enabled FROM authentication_policy WHERE singleton),
                'oidc_providers',COALESCE((SELECT jsonb_agg(jsonb_build_object('name',p.name,'enabled',p.enabled) ORDER BY p.name,p.id)
                    FROM oidc_identities i JOIN oidc_providers p ON p.id=i.provider_id WHERE i.actor_id=a.id),'[]'::jsonb),
                'workflow_name',(SELECT string_agg(w.name, ', ' ORDER BY w.name) FROM developer_workflows d JOIN workflows w ON w.id=d.workflow_id WHERE d.actor_id=a.id)
            )"#
}
