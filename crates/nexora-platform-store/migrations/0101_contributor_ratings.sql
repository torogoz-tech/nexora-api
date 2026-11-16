-- Personal reports never enter public transition run records or technical PR descriptions.
CREATE TABLE task_contributor_ratings (
 id text PRIMARY KEY,
 review_id text NOT NULL REFERENCES task_pre_pr_reviews(id),
 actor_id text NOT NULL REFERENCES actors(id),
 fingerprint text NOT NULL,
 input jsonb NOT NULL CHECK(octet_length(input::text)<=524288),
 status text NOT NULL CHECK(status IN ('running','completed','completed_no_score','reconcile_required','stale')),
 report jsonb CHECK(octet_length(report::text)<=65536),
 model text NOT NULL,
 rating_entry_id text REFERENCES developer_rating_entries(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 finished_at timestamptz,
 UNIQUE(review_id,actor_id),
 CHECK((status='completed')=(rating_entry_id IS NOT NULL))
);
