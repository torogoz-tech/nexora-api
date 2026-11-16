CREATE UNIQUE INDEX workflow_versions_project_identity ON workflow_versions(id,project_id);
CREATE TABLE project_documents (
 id text PRIMARY KEY,
 project_id text NOT NULL REFERENCES projects(id),
 code text NOT NULL CHECK(code ~ '^[a-z0-9_.-]{1,64}$'),
 kind text NOT NULL CHECK(kind IN ('requirement','business_rule','decision','document')),
 archived boolean NOT NULL DEFAULT false,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 latest_version_id text NOT NULL,
 latest_version_number bigint NOT NULL CHECK(latest_version_number>0),
 published_version_id text,
 created_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(id,project_id), UNIQUE(project_id,code)
);
CREATE INDEX project_documents_page ON project_documents(project_id,id);
CREATE TABLE project_document_versions (
 id text PRIMARY KEY,
 project_id text NOT NULL,
 document_id text NOT NULL,
 number bigint NOT NULL CHECK(number>0),
 previous_version_id text,
 title text NOT NULL CHECK(octet_length(title) BETWEEN 1 AND 160),
 content jsonb NOT NULL CHECK(jsonb_typeof(content)='object' AND octet_length(content::text)<=131072),
 origin text NOT NULL CHECK(origin IN ('observed_code','declared_business','agent_proposed')),
 workflow_version_id text NOT NULL,
 checksum text NOT NULL CHECK(checksum ~ '^[0-9a-f]{64}$'),
 reason text NOT NULL CHECK(octet_length(reason) BETWEEN 1 AND 4000),
 created_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(document_id,project_id) REFERENCES project_documents(id,project_id),
 FOREIGN KEY(workflow_version_id,project_id) REFERENCES workflow_versions(id,project_id),
 UNIQUE(document_id,number), UNIQUE(id,document_id,project_id),
 FOREIGN KEY(previous_version_id,document_id,project_id) REFERENCES project_document_versions(id,document_id,project_id)
);
ALTER TABLE project_documents
 ADD CONSTRAINT document_latest_version_fk FOREIGN KEY(latest_version_id,id,project_id) REFERENCES project_document_versions(id,document_id,project_id) DEFERRABLE INITIALLY DEFERRED,
 ADD CONSTRAINT document_published_version_fk FOREIGN KEY(published_version_id,id,project_id) REFERENCES project_document_versions(id,document_id,project_id);
CREATE TABLE document_version_states (
 version_id text PRIMARY KEY REFERENCES project_document_versions(id),
 state_id text NOT NULL,
 state_revision bigint NOT NULL DEFAULT 1 CHECK(state_revision>0),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE document_publications (
 id text PRIMARY KEY,
 project_id text NOT NULL,
 document_id text NOT NULL,
 version_id text NOT NULL,
 checksum text NOT NULL CHECK(checksum ~ '^[0-9a-f]{64}$'),
 workflow_version_id text NOT NULL,
 state_id text NOT NULL,
 state_revision bigint NOT NULL CHECK(state_revision>0),
 published_by text NOT NULL REFERENCES actors(id),
 reason text NOT NULL CHECK(octet_length(reason) BETWEEN 1 AND 4000),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(version_id,document_id,project_id) REFERENCES project_document_versions(id,document_id,project_id),
 FOREIGN KEY(workflow_version_id,project_id) REFERENCES workflow_versions(id,project_id),
 UNIQUE(version_id,state_revision)
);
CREATE TABLE document_history (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 project_id text NOT NULL,
 document_id text NOT NULL,
 version_id text NOT NULL,
 event text NOT NULL,
 actor_id text NOT NULL REFERENCES actors(id),
 credential_id text NOT NULL,
 reason text NOT NULL CHECK(octet_length(reason) BETWEEN 1 AND 4000),
 details jsonb NOT NULL CHECK(octet_length(details::text)<=16384),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(version_id,document_id,project_id) REFERENCES project_document_versions(id,document_id,project_id)
);
CREATE INDEX document_history_page ON document_history(document_id,id);
CREATE FUNCTION forbid_document_record_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='document content, publications and history are immutable';
END;
$$;
CREATE TRIGGER document_version_immutable BEFORE UPDATE OR DELETE ON project_document_versions FOR EACH ROW EXECUTE FUNCTION forbid_document_record_mutation();
CREATE TRIGGER document_publication_immutable BEFORE UPDATE OR DELETE ON document_publications FOR EACH ROW EXECUTE FUNCTION forbid_document_record_mutation();
CREATE TRIGGER document_history_immutable BEFORE UPDATE OR DELETE ON document_history FOR EACH ROW EXECUTE FUNCTION forbid_document_record_mutation();
CREATE FUNCTION validate_document_version_state() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS (
  SELECT 1 FROM project_document_versions v JOIN workflow_versions w ON w.id=v.workflow_version_id
  CROSS JOIN LATERAL jsonb_array_elements(w.graph->'states') s
  WHERE v.id=NEW.version_id AND s->>'id'=NEW.state_id
 ) THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='document state is outside its pinned workflow';
 END IF;
 RETURN NEW;
END;
$$;
CREATE TRIGGER document_state_valid BEFORE INSERT OR UPDATE ON document_version_states FOR EACH ROW EXECUTE FUNCTION validate_document_version_state();
