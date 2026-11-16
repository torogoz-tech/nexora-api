CREATE TABLE IF NOT EXISTS platform_schema_versions(version integer PRIMARY KEY, checksum text NOT NULL);
CREATE TABLE IF NOT EXISTS installation(
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton), id text UNIQUE NOT NULL,
 organization_id text UNIQUE NOT NULL, organization_name text NOT NULL, created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS actors(
 id text PRIMARY KEY, kind text NOT NULL CHECK(kind IN ('human','agent','integration')),
 name text NOT NULL, purpose text NOT NULL, active boolean NOT NULL DEFAULT true,
 revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS users(
 actor_id text PRIMARY KEY REFERENCES actors(id), username text NOT NULL UNIQUE, password_hash text,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS projects(
 id text PRIMARY KEY, name text NOT NULL, normalized_name text NOT NULL UNIQUE, description text,
 archived boolean NOT NULL DEFAULT false, revision bigint NOT NULL DEFAULT 1 CHECK(revision>0),
 created_at timestamptz NOT NULL DEFAULT now(), updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS actor_grants(
 actor_id text NOT NULL REFERENCES actors(id), capability text NOT NULL,
 project_id text REFERENCES projects(id),
 UNIQUE NULLS NOT DISTINCT(actor_id,capability,project_id)
);
CREATE TABLE IF NOT EXISTS api_keys(
 id text PRIMARY KEY, actor_id text NOT NULL REFERENCES actors(id), name text NOT NULL,
 digest bytea NOT NULL, scope jsonb NOT NULL, expires_at timestamptz NOT NULL,
 revoked_at timestamptz, created_at timestamptz NOT NULL DEFAULT now(), last_used_at timestamptz,
 CHECK(octet_length(digest)=32)
);
CREATE INDEX IF NOT EXISTS api_keys_owner ON api_keys(actor_id,id);
CREATE TABLE IF NOT EXISTS sessions(
 id text PRIMARY KEY, actor_id text NOT NULL REFERENCES users(actor_id), digest bytea NOT NULL,
 expires_at timestamptz NOT NULL, revoked_at timestamptz, created_at timestamptz NOT NULL DEFAULT now(),
 CHECK(octet_length(digest)=32)
);
CREATE TABLE IF NOT EXISTS audit_events(
 id text PRIMARY KEY, actor_id text REFERENCES actors(id), credential_id text,
 operation text NOT NULL, resource_id text, metadata jsonb NOT NULL DEFAULT '{}', created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE IF NOT EXISTS request_receipts(
 actor_id text NOT NULL REFERENCES actors(id), operation text NOT NULL, request_id text NOT NULL,
 request_hash text NOT NULL, response bytea NOT NULL, created_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(actor_id,operation,request_id)
);
