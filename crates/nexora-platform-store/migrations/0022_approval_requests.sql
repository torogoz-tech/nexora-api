CREATE TABLE document_approvals (
 id text PRIMARY KEY,
 project_id text NOT NULL,
 document_id text NOT NULL,
 version_id text NOT NULL,
 document_revision bigint NOT NULL CHECK(document_revision>0),
 state_revision bigint NOT NULL CHECK(state_revision>0),
 content_checksum text NOT NULL CHECK(content_checksum ~ '^[0-9a-f]{64}$'),
 workflow_version_id text NOT NULL,
 transition_id text NOT NULL,
 from_state text NOT NULL,
 to_state text NOT NULL,
 parameters jsonb NOT NULL CHECK(jsonb_typeof(parameters)='object' AND octet_length(parameters::text)<=32768),
 parameters_checksum text NOT NULL CHECK(parameters_checksum ~ '^[0-9a-f]{64}$'),
 reason text NOT NULL CHECK(octet_length(reason) BETWEEN 1 AND 4000),
 granted_by text NOT NULL REFERENCES actors(id),
 credential_id text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 expires_at timestamptz NOT NULL,
 consumed_at timestamptz,
 consumed_by text REFERENCES actors(id),
 consumed_credential_id text,
 revoked_at timestamptz,
 revoked_by text REFERENCES actors(id),
 revocation_reason text,
 FOREIGN KEY(version_id,document_id,project_id) REFERENCES project_document_versions(id,document_id,project_id),
 FOREIGN KEY(workflow_version_id,project_id) REFERENCES workflow_versions(id,project_id),
 UNIQUE(id,project_id),
 CHECK((consumed_at IS NULL AND consumed_by IS NULL AND consumed_credential_id IS NULL) OR (consumed_at IS NOT NULL AND consumed_by IS NOT NULL AND consumed_credential_id IS NOT NULL)),
 CHECK((revoked_at IS NULL AND revoked_by IS NULL AND revocation_reason IS NULL) OR (revoked_at IS NOT NULL AND revoked_by IS NOT NULL AND revocation_reason IS NOT NULL)),
 CHECK(consumed_at IS NULL OR revoked_at IS NULL)
);
CREATE INDEX document_approvals_resource ON document_approvals(project_id,document_id,id);
CREATE TABLE approval_requests (
 id text PRIMARY KEY,
 project_id text NOT NULL REFERENCES projects(id),
 target jsonb NOT NULL CHECK(jsonb_typeof(target)='object' AND octet_length(target::text)<=32768),
 target_kind text NOT NULL CHECK(target_kind IN ('task','document_version')),
 resource_id text NOT NULL,
 version_id text,
 transition_id text NOT NULL,
 root_revision bigint NOT NULL CHECK(root_revision>0),
 resource_revision bigint NOT NULL CHECK(resource_revision>0),
 workflow_version_id text NOT NULL,
 from_state text NOT NULL,
 to_state text NOT NULL,
 operation_checksum text NOT NULL CHECK(operation_checksum ~ '^[0-9a-f]{64}$'),
 status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','approved','rejected','cancelled')),
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 reason text NOT NULL CHECK(octet_length(reason) BETWEEN 1 AND 4000),
 requested_by text NOT NULL REFERENCES actors(id),
 credential_id text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 expires_at timestamptz NOT NULL,
 task_approval_id text REFERENCES task_approvals(id),
 document_approval_id text REFERENCES document_approvals(id),
 decided_by text REFERENCES actors(id),
 decided_credential_id text,
 decision_reason text,
 decided_at timestamptz,
 FOREIGN KEY(workflow_version_id,project_id) REFERENCES workflow_versions(id,project_id),
 CHECK((target_kind='task' AND version_id IS NULL) OR (target_kind='document_version' AND version_id IS NOT NULL)),
 CHECK((status='pending' AND decided_by IS NULL AND decided_credential_id IS NULL AND decision_reason IS NULL AND decided_at IS NULL AND task_approval_id IS NULL AND document_approval_id IS NULL) OR (status<>'pending' AND decided_by IS NOT NULL AND decided_credential_id IS NOT NULL AND decision_reason IS NOT NULL AND octet_length(decision_reason) BETWEEN 1 AND 4000 AND decided_at IS NOT NULL)),
 CHECK((status='approved' AND ((target_kind='task' AND task_approval_id IS NOT NULL AND document_approval_id IS NULL) OR (target_kind='document_version' AND task_approval_id IS NULL AND document_approval_id IS NOT NULL))) OR (status<>'approved' AND task_approval_id IS NULL AND document_approval_id IS NULL))
);
CREATE INDEX approval_request_page ON approval_requests(project_id,id);
CREATE INDEX approval_request_status_page ON approval_requests(project_id,status,id);
CREATE FUNCTION retain_approval_request_operation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='approval request history is retained'; END IF;
 IF OLD.status<>'pending' OR NEW.status='pending' OR NEW.revision<>OLD.revision+1 OR
 (to_jsonb(NEW)-ARRAY['status','revision','task_approval_id','document_approval_id','decided_by','decided_credential_id','decision_reason','decided_at']) IS DISTINCT FROM
 (to_jsonb(OLD)-ARRAY['status','revision','task_approval_id','document_approval_id','decided_by','decided_credential_id','decision_reason','decided_at']) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='approval request operation is immutable and decides only once';
 END IF;
 RETURN NEW;
END;
$$;
CREATE TRIGGER approval_request_operation BEFORE UPDATE OR DELETE ON approval_requests FOR EACH ROW EXECUTE FUNCTION retain_approval_request_operation();
