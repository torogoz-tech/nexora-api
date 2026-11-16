CREATE TABLE knowledge_images (
 id text PRIMARY KEY,
 project_id text REFERENCES projects(id),
 uploaded_by text NOT NULL REFERENCES actors(id),
 name text NOT NULL,
 payload bytea NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX knowledge_images_project ON knowledge_images(project_id);
ALTER TABLE knowledge_contributions DROP CONSTRAINT knowledge_contributions_check;
ALTER TABLE knowledge_contributions ADD CONSTRAINT knowledge_contributions_check CHECK (
 (kind='document' AND ((filename IS NOT NULL AND payload IS NOT NULL) OR (filename IS NULL AND payload IS NULL AND status='ready')))
 OR (kind='technical_debt' AND payload IS NULL)
);
