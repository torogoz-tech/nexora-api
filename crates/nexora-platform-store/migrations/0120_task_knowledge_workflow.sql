-- Task-transition AI work is queued for the knowledge worker and resumed by the release worker.
ALTER TABLE task_transition_runs
    ADD COLUMN wake_pending boolean NOT NULL DEFAULT false,
    ADD COLUMN wake_generation bigint NOT NULL DEFAULT 0;
CREATE INDEX task_transition_wakeup_pending
    ON task_transition_runs(updated_at, id)
    WHERE wake_pending AND record->>'status' = 'ready';

ALTER TABLE task_pre_pr_reviews
    DROP CONSTRAINT task_pre_pr_reviews_status_check;
ALTER TABLE task_pre_pr_reviews
    ADD CONSTRAINT task_pre_pr_reviews_status_check
    CHECK (status IN ('queued', 'running', 'completed', 'failed', 'stale'));
ALTER TABLE task_pre_pr_reviews
    ADD COLUMN lease_until timestamptz,
    ADD COLUMN claim_token text,
    ADD COLUMN retry_count integer NOT NULL DEFAULT 0,
    ADD COLUMN available_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    ADD COLUMN last_error_code text,
    ADD COLUMN last_error_message text,
    ADD CONSTRAINT task_pre_pr_reviews_retry_count_check CHECK (retry_count >= 0),
    ADD CONSTRAINT task_pre_pr_reviews_last_error_code_check CHECK (last_error_code IS NULL OR length(last_error_code) <= 96),
    ADD CONSTRAINT task_pre_pr_reviews_last_error_message_check CHECK (last_error_message IS NULL OR length(last_error_message) <= 2048);
UPDATE task_pre_pr_reviews
SET lease_until = '-infinity'::timestamptz
WHERE status = 'running';

ALTER TABLE task_contributor_ratings
    DROP CONSTRAINT task_contributor_ratings_status_check;
ALTER TABLE task_contributor_ratings
    ADD CONSTRAINT task_contributor_ratings_status_check
    CHECK (status IN (
        'queued', 'running', 'completed', 'completed_no_score',
        'reconcile_required', 'stale', 'failed'
    ));
ALTER TABLE task_contributor_ratings
    ADD COLUMN lease_until timestamptz,
    ADD COLUMN claim_token text,
    ADD COLUMN retry_count integer NOT NULL DEFAULT 0,
    ADD COLUMN available_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    ADD COLUMN last_error_code text,
    ADD COLUMN last_error_message text,
    ALTER COLUMN model DROP NOT NULL;
ALTER TABLE task_contributor_ratings
    ADD CONSTRAINT task_contributor_ratings_retry_count_check CHECK (retry_count >= 0),
    ADD CONSTRAINT task_contributor_ratings_last_error_code_check CHECK (last_error_code IS NULL OR length(last_error_code) <= 96),
    ADD CONSTRAINT task_contributor_ratings_last_error_message_check CHECK (last_error_message IS NULL OR length(last_error_message) <= 2048);
UPDATE task_contributor_ratings
SET lease_until = '-infinity'::timestamptz
WHERE status = 'running';

DROP INDEX task_pre_pr_review_claim;
CREATE UNIQUE INDEX task_pre_pr_review_claim
    ON task_pre_pr_reviews(run_id, repository_id, head_sha, base_sha)
    WHERE status IN ('queued', 'running', 'completed');

CREATE INDEX task_pre_pr_review_pending
    ON task_pre_pr_reviews(available_at, id)
    WHERE status = 'queued';
CREATE INDEX task_pre_pr_review_lease
    ON task_pre_pr_reviews(lease_until, id)
    WHERE status = 'running';
CREATE INDEX task_contributor_rating_pending
    ON task_contributor_ratings(available_at, id)
    WHERE status = 'queued';
CREATE INDEX task_contributor_rating_lease
    ON task_contributor_ratings(lease_until, id)
    WHERE status = 'running';
