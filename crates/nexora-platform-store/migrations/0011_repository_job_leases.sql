ALTER TABLE repository_jobs ADD COLUMN lease_owner text;
ALTER TABLE repository_jobs ADD COLUMN lease_token_digest bytea CHECK(lease_token_digest IS NULL OR octet_length(lease_token_digest)=32);
ALTER TABLE repository_jobs ADD COLUMN lease_until timestamptz;
ALTER TABLE repository_jobs ADD CONSTRAINT repository_job_lease_state CHECK(
 (state='running' AND lease_owner IS NOT NULL AND lease_token_digest IS NOT NULL AND lease_until IS NOT NULL)
 OR (state<>'running' AND lease_owner IS NULL AND lease_token_digest IS NULL AND lease_until IS NULL)
);
CREATE INDEX repository_job_expired ON repository_jobs(lease_until,id) WHERE state='running';
