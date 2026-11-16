CREATE TABLE document_links (
 id text PRIMARY KEY,
 project_id text NOT NULL,
 source_document_id text NOT NULL,
 source_version_id text NOT NULL,
 kind text NOT NULL CHECK(kind IN ('references','implements','required_for_transition')),
 target_task_id text,
 target_document_id text,
 target_version_id text,
 source_revision_at_creation bigint NOT NULL CHECK(source_revision_at_creation>0),
 target_revision_at_creation bigint NOT NULL CHECK(target_revision_at_creation>0),
 reason text NOT NULL CHECK(octet_length(reason) BETWEEN 1 AND 4000),
 created_by text NOT NULL REFERENCES actors(id),
 credential_id text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 removed_at timestamptz,
 removed_by text REFERENCES actors(id),
 removal_reason text,
 FOREIGN KEY(source_version_id,source_document_id,project_id) REFERENCES project_document_versions(id,document_id,project_id),
 FOREIGN KEY(target_task_id,project_id) REFERENCES tasks(id,project_id),
 FOREIGN KEY(target_version_id,target_document_id,project_id) REFERENCES project_document_versions(id,document_id,project_id),
 CHECK((target_task_id IS NOT NULL AND target_document_id IS NULL AND target_version_id IS NULL) OR (target_task_id IS NULL AND target_document_id IS NOT NULL AND target_version_id IS NOT NULL AND target_version_id<>source_version_id)),
 CHECK(kind<>'required_for_transition' OR target_task_id IS NOT NULL),
 CHECK((removed_at IS NULL AND removed_by IS NULL AND removal_reason IS NULL) OR (removed_at IS NOT NULL AND removed_by IS NOT NULL AND removal_reason IS NOT NULL AND octet_length(removal_reason) BETWEEN 1 AND 4000))
);
CREATE UNIQUE INDEX document_link_task_active ON document_links(source_version_id,target_task_id,kind) WHERE removed_at IS NULL AND target_task_id IS NOT NULL;
CREATE UNIQUE INDEX document_link_document_active ON document_links(source_version_id,target_version_id,kind) WHERE removed_at IS NULL AND target_version_id IS NOT NULL;
CREATE INDEX document_link_source_page ON document_links(project_id,source_document_id,id);
CREATE INDEX document_link_target_document_page ON document_links(project_id,target_document_id,id);
CREATE INDEX document_link_target_task_page ON document_links(project_id,target_task_id,id);
CREATE FUNCTION retain_document_link_identity() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='document link history cannot be deleted'; END IF;
 IF OLD.removed_at IS NOT NULL OR NEW.removed_at IS NULL OR NEW.removed_by IS NULL OR NEW.removal_reason IS NULL OR
  (to_jsonb(NEW)-ARRAY['removed_at','removed_by','removal_reason']) IS DISTINCT FROM (to_jsonb(OLD)-ARRAY['removed_at','removed_by','removal_reason']) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='document links only allow a single audited removal';
 END IF;
 RETURN NEW;
END;
$$;
CREATE TRIGGER document_link_identity BEFORE UPDATE OR DELETE ON document_links FOR EACH ROW EXECUTE FUNCTION retain_document_link_identity();
CREATE TABLE project_context_packages (
 id text PRIMARY KEY,
 project_id text NOT NULL REFERENCES projects(id),
 name text NOT NULL CHECK(octet_length(name) BETWEEN 1 AND 160),
 content jsonb NOT NULL CHECK(jsonb_typeof(content)='object' AND octet_length(content::text)<=1048576),
 checksum text NOT NULL CHECK(checksum ~ '^[0-9a-f]{64}$'),
 reason text NOT NULL CHECK(octet_length(reason) BETWEEN 1 AND 4000),
 created_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX project_context_packages_page ON project_context_packages(project_id,id);
CREATE TRIGGER context_package_immutable BEFORE UPDATE OR DELETE ON project_context_packages FOR EACH ROW EXECUTE FUNCTION forbid_document_record_mutation();
