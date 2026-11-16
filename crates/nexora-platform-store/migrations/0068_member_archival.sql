ALTER TABLE users ADD COLUMN archived_at timestamptz;
ALTER TABLE users ADD COLUMN archived_was_active boolean NOT NULL DEFAULT true;
ALTER TABLE users ADD COLUMN lifecycle_revision bigint NOT NULL DEFAULT 0 CHECK(lifecycle_revision>=0);
