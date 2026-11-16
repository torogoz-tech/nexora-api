ALTER TABLE user_security
    ADD COLUMN primary_auth_method text NOT NULL DEFAULT 'none'
    CHECK (primary_auth_method IN ('none', 'totp', 'passkey'));

UPDATE user_security
SET primary_auth_method = 'totp'
WHERE secret IS NOT NULL;

UPDATE user_security
SET primary_auth_method = 'passkey'
WHERE secret IS NULL
  AND EXISTS (
      SELECT 1
      FROM passkey_credentials
      WHERE passkey_credentials.actor_id = user_security.actor_id
  );
