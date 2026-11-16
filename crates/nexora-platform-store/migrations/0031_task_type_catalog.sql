CREATE TABLE task_type_catalog (
 code text PRIMARY KEY CHECK(code ~ '^[a-z0-9_-]{1,64}$'),
 name text NOT NULL CHECK(octet_length(name) BETWEEN 1 AND 160),
 description text CHECK(octet_length(description)<=2000),
 color text CHECK(color IS NULL OR color ~ '^#[0-9A-Fa-f]{6}$'),
 archived boolean NOT NULL DEFAULT false,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0)
);
-- Keep all project selections and task references intact. Conflicting legacy
-- labels remain in the project snapshots until an explicit catalog edit.
INSERT INTO task_type_catalog(code,name,description,color)
SELECT DISTINCT ON(code) code,name,description,color FROM task_types ORDER BY code,project_id;
ALTER TABLE task_types ADD CONSTRAINT task_type_catalog_fk FOREIGN KEY(code) REFERENCES task_type_catalog(code);
CREATE TABLE project_task_type_settings (
 project_id text PRIMARY KEY REFERENCES projects(id), revision bigint NOT NULL DEFAULT 0
);
CREATE FUNCTION task_type_settings_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO project_task_type_settings(project_id,revision) VALUES(COALESCE(NEW.project_id,OLD.project_id),1)
 ON CONFLICT(project_id) DO UPDATE SET revision=project_task_type_settings.revision+1;
 RETURN NULL;
END $$;
CREATE TRIGGER task_type_settings_change AFTER INSERT OR UPDATE OR DELETE ON task_types
FOR EACH ROW EXECUTE FUNCTION task_type_settings_revision();
