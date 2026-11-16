CREATE TABLE task_templates (
 id text PRIMARY KEY,
 project_id text NOT NULL REFERENCES projects(id),
 code text NOT NULL CHECK(code ~ '^[a-z0-9_.-]{1,64}$'),
 name text NOT NULL CHECK(octet_length(name) BETWEEN 1 AND 160),
 archived boolean NOT NULL DEFAULT false,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 latest_version_id text,
 latest_version_number bigint NOT NULL DEFAULT 0,
 created_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 UNIQUE(id,project_id),
 UNIQUE(project_id,code),
 CHECK((latest_version_id IS NULL AND latest_version_number=0) OR (latest_version_id IS NOT NULL AND latest_version_number>0))
);
CREATE INDEX task_templates_project ON task_templates(project_id,id);
CREATE TABLE task_template_versions (
 id text PRIMARY KEY,
 project_id text NOT NULL,
 template_id text NOT NULL,
 number bigint NOT NULL CHECK(number>0),
 previous_version_id text,
 code text NOT NULL,
 name text NOT NULL,
 definition jsonb NOT NULL CHECK(jsonb_typeof(definition)='object' AND octet_length(definition::text)<=65536),
 valid_from timestamptz NOT NULL,
 valid_until timestamptz,
 checksum text NOT NULL CHECK(length(checksum)=64),
 reason text NOT NULL CHECK(octet_length(reason) BETWEEN 1 AND 4000),
 published_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 CHECK(valid_until IS NULL OR valid_until>valid_from),
 FOREIGN KEY(template_id,project_id) REFERENCES task_templates(id,project_id),
 UNIQUE(template_id,number),
 UNIQUE(id,project_id),
 UNIQUE(id,template_id,project_id),
 FOREIGN KEY(previous_version_id,template_id,project_id) REFERENCES task_template_versions(id,template_id,project_id)
);
ALTER TABLE task_templates ADD CONSTRAINT task_template_latest_version_fk
 FOREIGN KEY(latest_version_id,id,project_id) REFERENCES task_template_versions(id,template_id,project_id);
CREATE FUNCTION forbid_task_template_version_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='published task template versions are immutable';
END;
$$;
CREATE TRIGGER task_template_version_immutable BEFORE UPDATE OR DELETE ON task_template_versions
 FOR EACH ROW EXECUTE FUNCTION forbid_task_template_version_mutation();
ALTER TABLE tasks
 ADD COLUMN template_version_id text,
 ADD COLUMN template_snapshot jsonb,
 ADD CONSTRAINT task_template_version_fk FOREIGN KEY(template_version_id,project_id) REFERENCES task_template_versions(id,project_id),
 ADD CONSTRAINT task_template_snapshot_identity CHECK(
  (template_version_id IS NULL AND template_snapshot IS NULL) OR
  (template_version_id IS NOT NULL AND template_snapshot IS NOT NULL AND jsonb_typeof(template_snapshot)='object'
   AND octet_length(template_snapshot::text)<=131072 AND template_snapshot ? 'id' AND template_snapshot ? 'project_id'
   AND COALESCE(template_snapshot->>'id'=template_version_id,false) AND COALESCE(template_snapshot->>'project_id'=project_id,false))
 );
CREATE FUNCTION forbid_task_template_snapshot_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.template_version_id IS DISTINCT FROM OLD.template_version_id OR NEW.template_snapshot IS DISTINCT FROM OLD.template_snapshot THEN
  RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='applied task template snapshot is immutable';
 END IF;
 RETURN NEW;
END;
$$;
CREATE TRIGGER task_template_snapshot_immutable BEFORE UPDATE OF template_version_id,template_snapshot ON tasks
 FOR EACH ROW EXECUTE FUNCTION forbid_task_template_snapshot_mutation();
