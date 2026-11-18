ALTER TABLE sessions DROP CONSTRAINT sessions_access_stage_check;
ALTER TABLE sessions ADD CONSTRAINT sessions_access_stage_check CHECK (
 access_stage IN ('ready','pending_role','totp_enroll','totp_verify','mfa_select','passkey_verify')
);
