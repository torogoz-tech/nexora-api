ALTER TABLE organization_documents DROP CONSTRAINT organization_documents_kind_check;
ALTER TABLE organization_documents ADD CONSTRAINT organization_documents_kind_check
 CHECK(kind IN ('document','requirement','business_rule','decision','best_practice','security','technical_debt'));
CREATE TABLE task_pre_pr_reviews (
 id text PRIMARY KEY,
 run_id text NOT NULL REFERENCES task_transition_runs(id),
 task_id text NOT NULL REFERENCES tasks(id),
 task_revision bigint NOT NULL,
 project_id text NOT NULL REFERENCES projects(id),
 repository_id text NOT NULL REFERENCES project_repositories(id),
 head_sha text NOT NULL,
 base_sha text NOT NULL,
 requested_by text NOT NULL REFERENCES actors(id),
 authority jsonb NOT NULL,
 input jsonb NOT NULL CHECK(octet_length(input::text)<=524288),
 status text NOT NULL CHECK(status IN ('running','completed','failed','stale')),
 report jsonb CHECK(octet_length(report::text)<=131072),
 model text,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 finished_at timestamptz
);
CREATE INDEX task_pre_pr_review_revision ON task_pre_pr_reviews(task_id,task_revision,repository_id,head_sha,base_sha,created_at DESC);

CREATE UNIQUE INDEX task_pre_pr_review_claim ON task_pre_pr_reviews(run_id,repository_id,head_sha,base_sha) WHERE status IN ('running','completed');
