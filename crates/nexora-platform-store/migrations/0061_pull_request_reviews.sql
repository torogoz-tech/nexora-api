CREATE TABLE pull_request_reviews (
 id text PRIMARY KEY,
 project_id text NOT NULL REFERENCES projects(id),
 repository_id text NOT NULL REFERENCES project_repositories(id),
 pr_number bigint NOT NULL CHECK(pr_number>0),
 requested_by text NOT NULL REFERENCES actors(id),
 request_id text NOT NULL CHECK(length(request_id) BETWEEN 8 AND 128),
 expected_head_sha text NOT NULL,
 source jsonb NOT NULL,
 input jsonb NOT NULL CHECK(octet_length(input::text)<=262144),
 status text NOT NULL CHECK(status IN ('running','completed','failed','stale')),
 report jsonb CHECK(octet_length(report::text)<=131072),
 model text,
 created_at timestamptz NOT NULL DEFAULT now(),
 finished_at timestamptz,
 UNIQUE(requested_by,request_id)
);
CREATE INDEX pull_request_reviews_history ON pull_request_reviews(repository_id,pr_number,created_at DESC);
