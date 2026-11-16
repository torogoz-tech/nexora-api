CREATE TABLE user_security(
 actor_id text PRIMARY KEY REFERENCES users(actor_id),
 pending_role boolean NOT NULL DEFAULT false,
 totp_exempt boolean NOT NULL DEFAULT false,
 skipped integer NOT NULL DEFAULT 0 CHECK(skipped BETWEEN 0 AND 5),
 secret jsonb,
 last_step bigint NOT NULL DEFAULT -1,
 failed_attempts integer NOT NULL DEFAULT 0,
 blocked_until timestamptz
);
ALTER TABLE sessions ADD COLUMN access_stage text NOT NULL DEFAULT 'ready'
 CHECK(access_stage IN ('ready','pending_role','totp_enroll','totp_verify'));
ALTER TABLE sessions ADD COLUMN setup_secret jsonb;
