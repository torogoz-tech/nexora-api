CREATE TABLE task_documents (
 id text PRIMARY KEY,
 task_id text NOT NULL REFERENCES tasks(id),
 name text NOT NULL CHECK(octet_length(name) BETWEEN 1 AND 240),
 payload bytea NOT NULL CHECK(octet_length(payload) BETWEEN 1 AND 20971520),
 sha256 text NOT NULL,
 uploaded_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX task_documents_task_id ON task_documents(task_id,id);
