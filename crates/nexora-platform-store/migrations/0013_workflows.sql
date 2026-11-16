CREATE TABLE workflows (
 id text PRIMARY KEY,
 project_id text NOT NULL REFERENCES projects(id),
 name text NOT NULL CHECK(octet_length(name) BETWEEN 1 AND 160),
 purpose text NOT NULL CHECK(octet_length(purpose) BETWEEN 1 AND 4000),
 resource_kinds jsonb NOT NULL CHECK(jsonb_typeof(resource_kinds)='array' AND jsonb_array_length(resource_kinds) BETWEEN 1 AND 5),
 draft jsonb NOT NULL CHECK(jsonb_typeof(draft)='object' AND octet_length(draft::text)<=524288),
 archived boolean NOT NULL DEFAULT false,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 latest_version_id text,
 latest_version_number bigint NOT NULL DEFAULT 0 CHECK(latest_version_number>=0),
 created_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(id,project_id),
 CHECK((latest_version_id IS NULL)=(latest_version_number=0))
);
CREATE INDEX workflows_project_page ON workflows(project_id,id);
CREATE TABLE workflow_versions (
 id text PRIMARY KEY,
 workflow_id text NOT NULL,
 project_id text NOT NULL,
 number bigint NOT NULL CHECK(number>0),
 name text NOT NULL CHECK(octet_length(name) BETWEEN 1 AND 160),
 purpose text NOT NULL CHECK(octet_length(purpose) BETWEEN 1 AND 4000),
 resource_kinds jsonb NOT NULL CHECK(jsonb_typeof(resource_kinds)='array' AND jsonb_array_length(resource_kinds) BETWEEN 1 AND 5),
 graph jsonb NOT NULL CHECK(jsonb_typeof(graph)='object' AND octet_length(graph::text)<=524288),
 checksum text NOT NULL CHECK(checksum ~ '^[0-9a-f]{64}$'),
 reason text NOT NULL CHECK(octet_length(reason) BETWEEN 1 AND 4000),
 published_by text NOT NULL REFERENCES actors(id),
 published_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 FOREIGN KEY(workflow_id,project_id) REFERENCES workflows(id,project_id),
 UNIQUE(workflow_id,number),
 UNIQUE(id,workflow_id,project_id)
);
ALTER TABLE workflows ADD CONSTRAINT workflow_latest_version_fk
 FOREIGN KEY(latest_version_id,id,project_id) REFERENCES workflow_versions(id,workflow_id,project_id);
CREATE FUNCTION forbid_workflow_version_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 RAISE EXCEPTION USING ERRCODE='55000',MESSAGE='published workflow versions are immutable';
END;
$$;
CREATE TRIGGER workflow_version_immutable BEFORE UPDATE OR DELETE ON workflow_versions
 FOR EACH ROW EXECUTE FUNCTION forbid_workflow_version_mutation();
