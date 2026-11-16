ALTER TABLE oidc_identities ADD COLUMN contact_email text;
ALTER TABLE oidc_identities ADD COLUMN contact_email_verified boolean NOT NULL DEFAULT false;
CREATE TABLE user_email_changes(
 id text PRIMARY KEY,actor_id text NOT NULL REFERENCES users(actor_id),email text NOT NULL,
 previous_email text NOT NULL,digest bytea NOT NULL,token_secret jsonb,
 attempts integer NOT NULL DEFAULT 0,used_at timestamptz,expires_at timestamptz NOT NULL,
 created_at timestamptz NOT NULL DEFAULT clock_timestamp(),CHECK(octet_length(digest)=32)
);
CREATE INDEX user_email_changes_actor ON user_email_changes(actor_id,created_at);
ALTER TABLE email_notifications DROP CONSTRAINT email_notifications_kind_check;
ALTER TABLE email_notifications ADD CONSTRAINT email_notifications_kind_check CHECK(kind IN ('assignment','approval','test','invitation','invitation_accepted','password_reset','approval_result','email_verification','email_changed'));
ALTER TABLE installation_email_contents DROP CONSTRAINT installation_email_contents_kind_check;
ALTER TABLE installation_email_contents ADD CONSTRAINT installation_email_contents_kind_check CHECK(kind IN ('invitation','invitation_accepted','password_reset','assignment','approval','approval_result','test','email_verification','email_changed'));
INSERT INTO installation_email_contents(kind,language,subject,body_html) VALUES('email_verification','es','Verifica tu correo en {{organization_name}}','<p>Hola {{recipient_name}},</p><p>Introduce este código en tu perfil para confirmar tu correo:</p><p><strong>{{verification_code}}</strong></p><p>Vence en 15 minutos. Si no lo solicitaste, ignora este mensaje.</p>');
INSERT INTO installation_email_contents(kind,language,subject,body_html) VALUES('email_verification','en','Verify your email at {{organization_name}}','<p>Hello {{recipient_name}},</p><p>Enter this code in your profile to confirm your email:</p><p><strong>{{verification_code}}</strong></p><p>It expires in 15 minutes. If you did not request this, ignore this message.</p>');
INSERT INTO installation_email_contents(kind,language,subject,body_html) VALUES('email_changed','es','Tu correo fue actualizado en {{organization_name}}','<p>Hola {{recipient_name}},</p><p>El correo de tu cuenta ahora es {{new_email}}. Si no solicitaste este cambio, comunícate con el administrador.</p>');
INSERT INTO installation_email_contents(kind,language,subject,body_html) VALUES('email_changed','en','Your email was updated at {{organization_name}}','<p>Hello {{recipient_name}},</p><p>Your account email is now {{new_email}}. If you did not request this change, contact your administrator.</p>');
