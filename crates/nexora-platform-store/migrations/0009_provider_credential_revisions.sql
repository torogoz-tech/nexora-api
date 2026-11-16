ALTER TABLE github_settings ADD COLUMN credential_revision bigint NOT NULL DEFAULT 1 CHECK(credential_revision>0);
UPDATE github_settings SET credential_revision=revision;
ALTER TABLE gitlab_settings ADD COLUMN credential_revision bigint NOT NULL DEFAULT 1 CHECK(credential_revision>0);
UPDATE gitlab_settings SET credential_revision=revision;
