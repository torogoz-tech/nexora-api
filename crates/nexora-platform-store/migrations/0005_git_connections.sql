CREATE TABLE gitlab_settings(
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),origin text NOT NULL,host text NOT NULL,client_id text NOT NULL,
 client_secret jsonb NOT NULL,redirect_uri text NOT NULL,enabled boolean NOT NULL,revision bigint NOT NULL CHECK(revision>0)
);
CREATE TABLE git_authorizations(
 id text PRIMARY KEY,provider text NOT NULL CHECK(provider IN ('github','gitlab')),config_revision bigint NOT NULL,
 owner_actor_id text NOT NULL REFERENCES actors(id),credential_id text NOT NULL,credential_method text NOT NULL CHECK(credential_method IN ('session','api_key')),
 state_digest bytea NOT NULL CHECK(octet_length(state_digest)=32),cookie_digest bytea NOT NULL CHECK(octet_length(cookie_digest)=32),
 verifier jsonb NOT NULL,expires_at timestamptz NOT NULL,consumed_at timestamptz,created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX git_authorization_expiry ON git_authorizations(expires_at);
CREATE TABLE git_connections(
 id text PRIMARY KEY,provider text NOT NULL CHECK(provider IN ('github','gitlab')),host text NOT NULL,
 owner_actor_id text NOT NULL REFERENCES actors(id),remote_account_id text NOT NULL,remote_login text NOT NULL,
 status text NOT NULL CHECK(status IN ('active','revoked')),revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 config_revision bigint NOT NULL,credential_revision bigint NOT NULL DEFAULT 1 CHECK(credential_revision>0),tokens jsonb NOT NULL,expires_at timestamptz NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now(),updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX git_connection_owner ON git_connections(owner_actor_id,id);
CREATE TABLE git_connection_projects(
 connection_id text NOT NULL REFERENCES git_connections(id),project_id text NOT NULL REFERENCES projects(id),
 granted_by text NOT NULL REFERENCES actors(id),created_at timestamptz NOT NULL DEFAULT now(),PRIMARY KEY(connection_id,project_id)
);
ALTER TABLE project_repositories ADD CONSTRAINT repository_authorized_connection FOREIGN KEY(connection_id) REFERENCES git_connections(id);
