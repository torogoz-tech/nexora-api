#!/usr/bin/env bash
# Validate upgrading the historical CHECK without granting access to unknown stages.
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
name="nexora-session-stage-test-$$"
cleanup() { docker rm -fv "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT
docker run -d --name "$name" --network none --memory 256m --tmpfs /var/lib/postgresql/data:rw,size=128m \
  -e POSTGRES_HOST_AUTH_METHOD=trust postgres:17-alpine \
  -c shared_buffers=16MB -c max_connections=10 >/dev/null
ready=false
for _ in {1..40}; do
  if docker exec "$name" pg_isready -U postgres >/dev/null 2>&1; then ready=true; break; fi
  sleep .5
done
[[ $ready == true ]]
{
cat <<'SQL'
BEGIN;
CREATE TEMP TABLE sessions (
 id integer PRIMARY KEY,
 access_stage text NOT NULL CONSTRAINT sessions_access_stage_check
 CHECK(access_stage IN ('ready','pending_role','totp_enroll','totp_verify'))
);
INSERT INTO sessions VALUES(1,'ready'),(2,'pending_role'),(3,'totp_enroll'),(4,'totp_verify');
DO $$
BEGIN
 BEGIN
  INSERT INTO sessions VALUES(5,'mfa_select');
  RAISE EXCEPTION 'Historical constraint unexpectedly accepted mfa_select';
 EXCEPTION WHEN check_violation THEN NULL;
 END;
 BEGIN
  INSERT INTO sessions VALUES(6,'passkey_verify');
  RAISE EXCEPTION 'Historical constraint unexpectedly accepted passkey_verify';
 EXCEPTION WHEN check_violation THEN NULL;
 END;
END $$;
SQL
cat "$root/crates/nexora-platform-store/migrations/0118_session_mfa_stages.sql"
cat <<'SQL'
INSERT INTO sessions VALUES(5,'mfa_select'),(6,'passkey_verify');
DO $$
BEGIN
 IF (SELECT count(*) FROM sessions)<>6 THEN RAISE EXCEPTION 'Existing stages were not preserved'; END IF;
 BEGIN
  INSERT INTO sessions VALUES(7,'bypass');
  RAISE EXCEPTION 'Unknown stage was accepted';
 EXCEPTION WHEN check_violation THEN NULL;
 END;
END $$;
ROLLBACK;
SQL
} | docker exec -i "$name" psql -U postgres -v ON_ERROR_STOP=1 -q
echo 'Session stage migration preserves existing states and rejects unknown states.'
