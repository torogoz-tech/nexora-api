CREATE TABLE authentication_policy(
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),local_enabled boolean NOT NULL DEFAULT true,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0)
);
INSERT INTO authentication_policy(singleton) VALUES(true);
CREATE TABLE oidc_providers(
 id text PRIMARY KEY,name text NOT NULL,issuer text NOT NULL UNIQUE,client_id text NOT NULL,
 client_secret jsonb NOT NULL,redirect_uri text NOT NULL,enabled boolean NOT NULL,
 revision bigint NOT NULL CHECK(revision>0),created_at timestamptz NOT NULL DEFAULT now(),updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE oidc_identities(
 issuer text NOT NULL,subject text NOT NULL,provider_id text NOT NULL REFERENCES oidc_providers(id),
 actor_id text NOT NULL REFERENCES users(actor_id),created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(issuer,subject),UNIQUE(actor_id,provider_id)
);
CREATE TABLE oidc_authorizations(
 id text PRIMARY KEY,provider_id text NOT NULL REFERENCES oidc_providers(id),provider_revision bigint NOT NULL,
 state_digest bytea NOT NULL CHECK(octet_length(state_digest)=32),cookie_digest bytea NOT NULL CHECK(octet_length(cookie_digest)=32),
 pending_secrets jsonb NOT NULL,expires_at timestamptz NOT NULL,consumed_at timestamptz,created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX oidc_authorization_expiry ON oidc_authorizations(expires_at);
ALTER TABLE sessions ADD COLUMN authentication_provider_id text REFERENCES oidc_providers(id);
CREATE INDEX sessions_authentication_provider ON sessions(authentication_provider_id) WHERE authentication_provider_id IS NOT NULL;
