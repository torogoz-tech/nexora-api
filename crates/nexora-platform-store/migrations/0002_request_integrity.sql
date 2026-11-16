CREATE TABLE IF NOT EXISTS platform_crypto_state(
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton), key_version integer NOT NULL CHECK(key_version=1), key_id text NOT NULL
);
