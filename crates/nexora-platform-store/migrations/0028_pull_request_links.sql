CREATE TABLE pull_request_links (
 id text PRIMARY KEY,
 project_id text NOT NULL REFERENCES projects(id),
 repository_id text NOT NULL,
 remote_id text NOT NULL,
 pull_request_number bigint NOT NULL CHECK(pull_request_number>0),
 kind text NOT NULL CHECK(kind IN ('task','requirement')),
 task_id text,
 document_id text,
 version_id text,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 created_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 removed_at timestamptz,
 CHECK((kind='task' AND task_id IS NOT NULL AND document_id IS NULL AND version_id IS NULL) OR (kind='requirement' AND task_id IS NULL AND document_id IS NOT NULL AND version_id IS NOT NULL)),
 FOREIGN KEY(repository_id,project_id) REFERENCES project_repositories(id,project_id),
 FOREIGN KEY(task_id,project_id) REFERENCES tasks(id,project_id),
 FOREIGN KEY(document_id,project_id) REFERENCES project_documents(id,project_id),
 FOREIGN KEY(version_id,document_id,project_id) REFERENCES project_document_versions(id,document_id,project_id)
);
CREATE UNIQUE INDEX pull_request_links_active ON pull_request_links(repository_id,remote_id,pull_request_number,kind,COALESCE(task_id,document_id),COALESCE(version_id,'')) WHERE removed_at IS NULL;
CREATE INDEX pull_request_links_page ON pull_request_links(project_id,repository_id,pull_request_number,id);
