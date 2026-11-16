CREATE TABLE project_context_manifests (
 id text PRIMARY KEY,
 project_id text NOT NULL REFERENCES projects(id),
 name text NOT NULL CHECK(octet_length(name) BETWEEN 1 AND 160),
 members jsonb NOT NULL CHECK(jsonb_typeof(members)='array' AND jsonb_array_length(members) BETWEEN 1 AND 64 AND octet_length(members::text)<=131072),
 checksum text NOT NULL CHECK(checksum ~ '^[0-9a-f]{64}$'),
 reason text NOT NULL CHECK(octet_length(reason) BETWEEN 1 AND 4000),
 created_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(id,project_id)
);
CREATE INDEX project_context_manifests_page ON project_context_manifests(project_id,id);
CREATE TRIGGER project_context_manifest_immutable BEFORE UPDATE OR DELETE ON project_context_manifests FOR EACH ROW EXECUTE FUNCTION forbid_document_record_mutation();
ALTER TABLE project_document_versions
 ADD COLUMN context_manifest_id text,
 ADD COLUMN evidence jsonb NOT NULL DEFAULT '[]' CHECK(jsonb_typeof(evidence)='array' AND jsonb_array_length(evidence)<=8 AND octet_length(evidence::text)<=65536),
 ADD CONSTRAINT document_context_manifest_fk FOREIGN KEY(context_manifest_id,project_id) REFERENCES project_context_manifests(id,project_id),
 ADD CONSTRAINT document_evidence_context CHECK(context_manifest_id IS NOT NULL OR evidence='[]'::jsonb);

CREATE FUNCTION project_context_status(p_project text,p_manifest text) RETURNS jsonb LANGUAGE SQL STABLE AS $$
 WITH members AS (
  SELECT item FROM project_context_manifests m CROSS JOIN LATERAL jsonb_array_elements(m.members) item WHERE m.id=p_manifest AND m.project_id=p_project
 ), sources AS (
  SELECT item,b.snapshot_id,b.payload_hash,b.pinned_commit,b.source_generation,b.connection_id,b.connection_revision,
   COALESCE(b.repository_id IS NOT NULL AND r.id IS NOT NULL AND NOT r.archived AND r.authorization_status='authorized' AND r.source_generation=b.source_generation AND r.connection_id=b.connection_id AND c.status='active' AND c.revision=b.connection_revision AND a.active AND i.singleton AND b.runtime_repository_id=b.repository_id,false) AS available
  FROM members
  LEFT JOIN repository_bindings b ON b.repository_id=item->>'repository_id' AND b.project_id=p_project
  LEFT JOIN project_repositories r ON r.id=b.repository_id AND r.project_id=b.project_id
  LEFT JOIN git_connections c ON c.id=b.connection_id
  LEFT JOIN actors a ON a.id=c.owner_actor_id
  LEFT JOIN installation i ON i.id=b.runtime_tenant_id
 ), classified AS (
  SELECT item->>'repository_id' AS repository_id,NOT available AS unavailable,
   available AND (snapshot_id IS DISTINCT FROM item->>'snapshot_id' OR payload_hash IS DISTINCT FROM item->>'payload_hash' OR pinned_commit IS DISTINCT FROM item->>'pinned_commit' OR source_generation IS DISTINCT FROM (item->>'source_generation')::bigint OR connection_id IS DISTINCT FROM item->>'connection_id' OR connection_revision IS DISTINCT FROM (item->>'connection_revision')::bigint) AS changed
  FROM sources
 )
 SELECT jsonb_build_object('manifest_id',p_manifest,'needs_review',COALESCE(bool_or(unavailable OR changed),false),'changed_repository_ids',COALESCE(jsonb_agg(repository_id ORDER BY repository_id) FILTER(WHERE changed),'[]'::jsonb),'unavailable_repository_ids',COALESCE(jsonb_agg(repository_id ORDER BY repository_id) FILTER(WHERE unavailable),'[]'::jsonb)) FROM classified;
$$;
