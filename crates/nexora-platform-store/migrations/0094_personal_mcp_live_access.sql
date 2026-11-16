-- Personal MCP keys inherit their owner's live authority. Generic installation
-- keys retain their explicitly delegated scope. Preserve read-only envelopes;
-- no digest, expiry, revocation or audit identity is changed.
UPDATE api_keys k
SET scope = jsonb_build_object(
    'capabilities', to_jsonb(CASE WHEN EXISTS (
        SELECT 1 FROM jsonb_array_elements_text(k.scope->'capabilities') c(value)
        WHERE c.value = ANY(ARRAY['knowledge:contribute', 'analysis-profiles:contribute', 'organization-knowledge:write', 'projects:write', 'repositories:write', 'repositories:sync', 'knowledge:write', 'tasks:write', 'tasks:transition', 'backlogs:write', 'documents:write', 'documents:publish']::text[])
    ) THEN ARRAY['members:read', 'connections:read', 'task-types:read', 'analysis-profiles:read', 'organization-knowledge:read', 'projects:read', 'repositories:read', 'knowledge:read', 'workflows:read', 'tasks:read', 'documents:read', 'knowledge:contribute', 'analysis-profiles:contribute', 'organization-knowledge:write', 'projects:write', 'repositories:write', 'repositories:sync', 'knowledge:write', 'tasks:write', 'tasks:transition', 'backlogs:write', 'documents:write', 'documents:publish']::text[] ELSE ARRAY['members:read', 'connections:read', 'task-types:read', 'analysis-profiles:read', 'organization-knowledge:read', 'projects:read', 'repositories:read', 'knowledge:read', 'workflows:read', 'tasks:read', 'documents:read']::text[] END),
    'project_ids', '[]'::jsonb,
    'all_projects', true,
    'installation', true
)
WHERE personal_mcp AND revoked_at IS NULL AND expires_at > clock_timestamp();
