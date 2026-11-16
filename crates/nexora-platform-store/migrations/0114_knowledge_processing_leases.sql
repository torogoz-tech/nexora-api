ALTER TABLE knowledge_processing_jobs
    ADD COLUMN IF NOT EXISTS lease_owner text,
    ADD COLUMN IF NOT EXISTS lease_token_digest bytea,
    ADD COLUMN IF NOT EXISTS lease_until timestamptz;

CREATE INDEX IF NOT EXISTS knowledge_processing_leases
    ON knowledge_processing_jobs(status,lease_until,created_at,id);
