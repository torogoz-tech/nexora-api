-- Keep historical workflow references and content checksums untouched.
-- New knowledge revisions are independent of task execution workflows.
ALTER TABLE project_document_versions ALTER COLUMN workflow_version_id DROP NOT NULL;
ALTER TABLE document_publications ALTER COLUMN workflow_version_id DROP NOT NULL;

CREATE OR REPLACE FUNCTION validate_document_version_state() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS (
  SELECT 1 FROM project_document_versions v
  WHERE v.id=NEW.version_id AND v.workflow_version_id IS NULL AND NEW.state_id='draft'
 ) THEN
  RETURN NEW;
 END IF;
 IF NOT EXISTS (
  SELECT 1 FROM project_document_versions v JOIN workflow_versions w ON w.id=v.workflow_version_id
  CROSS JOIN LATERAL jsonb_array_elements(w.graph->'states') s
  WHERE v.id=NEW.version_id AND s->>'id'=NEW.state_id
 ) THEN
  RAISE EXCEPTION USING ERRCODE='23514', MESSAGE='invalid document version state';
 END IF;
 RETURN NEW;
END;
$$;
