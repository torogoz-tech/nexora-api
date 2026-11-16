CREATE TABLE organization_stickers (
 id text PRIMARY KEY,
 name text NOT NULL CHECK(length(name) BETWEEN 1 AND 80),
 payload bytea NOT NULL CHECK(octet_length(payload) BETWEEN 1 AND 8388608),
 sha256 text NOT NULL,
 archived boolean NOT NULL DEFAULT false,
 created_by text NOT NULL REFERENCES actors(id),
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
