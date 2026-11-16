CREATE TABLE task_evidence (
 id text PRIMARY KEY, uploaded_by text NOT NULL REFERENCES actors(id),
 task_id text REFERENCES tasks(id), name text NOT NULL CHECK(octet_length(name) BETWEEN 1 AND 200),
 payload bytea NOT NULL CHECK(octet_length(payload) BETWEEN 1 AND 8388608),
 sha256 text NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX task_evidence_task ON task_evidence(task_id,id);
CREATE INDEX task_evidence_drafts ON task_evidence(uploaded_by,created_at) WHERE task_id IS NULL;

CREATE TABLE task_feature_associations (
 task_id text NOT NULL REFERENCES tasks(id), feature_id text NOT NULL REFERENCES project_features(id),
 linked_by text NOT NULL REFERENCES actors(id), created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 PRIMARY KEY(task_id,feature_id)
);
