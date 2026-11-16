CREATE TABLE evidence_files (
    id text PRIMARY KEY,
    project_id text NOT NULL REFERENCES projects(id),
    repository_id text NOT NULL REFERENCES project_repositories(id),
    snapshot_id text,
    document_id text,
    document_version_id text,
    payload bytea NOT NULL CHECK (octet_length(payload) BETWEEN 1 AND 4194304),
    record jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK ((document_id IS NULL) = (document_version_id IS NULL))
);
CREATE INDEX evidence_files_project_repository ON evidence_files(project_id,repository_id,id);
CREATE INDEX evidence_files_document_version ON evidence_files(project_id,document_id,document_version_id,id);
