CREATE TABLE github_manifest_authorizations(
 id text PRIMARY KEY,owner_actor_id text NOT NULL REFERENCES actors(id),credential_id text NOT NULL,
 credential_method text NOT NULL CHECK(credential_method IN ('session','api_key')),
 state_digest bytea NOT NULL CHECK(octet_length(state_digest)=32),cookie_digest bytea NOT NULL CHECK(octet_length(cookie_digest)=32),
 expected_revision bigint NOT NULL CHECK(expected_revision>=0),origin text NOT NULL,api_origin text NOT NULL,callback_origin text NOT NULL,
 expires_at timestamptz NOT NULL,consumed_at timestamptz,created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX github_manifest_expiry ON github_manifest_authorizations(expires_at);
