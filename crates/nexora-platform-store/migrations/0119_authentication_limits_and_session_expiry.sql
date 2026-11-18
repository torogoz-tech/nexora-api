CREATE TABLE authentication_attempt_limits(
 key_digest bytea PRIMARY KEY CHECK(octet_length(key_digest)=32),
 window_started_at timestamptz NOT NULL,
 attempts integer NOT NULL CHECK(attempts>0)
);
CREATE INDEX authentication_attempt_limits_expiry_idx ON authentication_attempt_limits(window_started_at);

ALTER TABLE sessions ADD COLUMN absolute_expires_at timestamptz;
ALTER TABLE sessions ADD COLUMN last_activity_at timestamptz;
UPDATE sessions SET absolute_expires_at=expires_at,last_activity_at=clock_timestamp();
ALTER TABLE sessions ALTER COLUMN absolute_expires_at SET NOT NULL;
ALTER TABLE sessions ALTER COLUMN absolute_expires_at SET DEFAULT (clock_timestamp()+interval '8 hours');
ALTER TABLE sessions ALTER COLUMN last_activity_at SET NOT NULL;
ALTER TABLE sessions ALTER COLUMN last_activity_at SET DEFAULT clock_timestamp();
