CREATE TABLE rating_evidence (
 id text PRIMARY KEY,
 target_id text NOT NULL REFERENCES actors(id),
 project_id text NOT NULL REFERENCES projects(id),
 uploaded_by text NOT NULL REFERENCES actors(id),
 rating_id text REFERENCES developer_rating_entries(id),
 name text NOT NULL CHECK(octet_length(name) BETWEEN 1 AND 240),
 image boolean NOT NULL,
 payload bytea NOT NULL CHECK(octet_length(payload) BETWEEN 1 AND 20971520),
 sha256 text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX rating_evidence_entry ON rating_evidence(rating_id);
