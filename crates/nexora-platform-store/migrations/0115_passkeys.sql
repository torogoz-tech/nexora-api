CREATE TABLE passkey_credentials(
 id text PRIMARY KEY,
 actor_id text NOT NULL REFERENCES users(actor_id),
 credential_id bytea NOT NULL UNIQUE,
 public_key_cose bytea NOT NULL,
 sign_count bigint NOT NULL DEFAULT 0,
 label text NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 last_used_at timestamptz
);

CREATE TABLE passkey_ceremonies(
 id text PRIMARY KEY,
 actor_id text NOT NULL REFERENCES users(actor_id),
 kind text NOT NULL CHECK(kind IN ('registration','authentication')),
 state jsonb NOT NULL,
 expires_at timestamptz NOT NULL,
 consumed_at timestamptz
);

CREATE INDEX passkey_credentials_actor_idx ON passkey_credentials(actor_id);
CREATE INDEX passkey_ceremonies_active_idx ON passkey_ceremonies(actor_id, expires_at) WHERE consumed_at IS NULL;
