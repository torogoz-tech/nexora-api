use crate::{PlatformStore, SecretGenerator, audit_writer::AuditWriter};
use nexora_platform::{ActorKind, AuthContext, AuthMethod, PlatformError};
use serde_json::{Value, json};
use subtle::ConstantTimeEq;
impl PlatformStore {
    pub fn own_email(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        let ctx = self.fresh(ctx)?;
        if ctx.method() != AuthMethod::Session || ctx.kind() != ActorKind::Human {
            return Err(PlatformError::Forbidden);
        }
        let row=self.client.query_one("SELECT COALESCE((SELECT email FROM notification_recipients WHERE actor_id=$1),''),(SELECT contact_email FROM oidc_identities i JOIN oidc_providers p ON p.id=i.provider_id WHERE i.actor_id=$1 AND p.enabled AND contact_email IS NOT NULL ORDER BY i.provider_id LIMIT 1),(SELECT email FROM user_email_changes WHERE actor_id=$1 AND used_at IS NULL AND expires_at>clock_timestamp() ORDER BY created_at DESC LIMIT 1)",&[&ctx.actor_id()]).map_err(Self::database_error)?;
        Ok(
            json!({"email":row.get::<_,String>(0),"suggested_email":row.get::<_,Option<String>>(1),"pending_email":row.get::<_,Option<String>>(2)}),
        )
    }
    pub fn request_own_email(
        &mut self,
        ctx: &AuthContext,
        email: &str,
    ) -> Result<(), PlatformError> {
        let ctx = self.fresh(ctx)?;
        if ctx.method() != AuthMethod::Session || ctx.kind() != ActorKind::Human {
            return Err(PlatformError::Forbidden);
        }
        let email = email.trim().to_ascii_lowercase();
        if email.len() > 254
            || !email_address::EmailAddress::is_valid(&email)
            || email.chars().any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(PlatformError::Validation("invalid email address"));
        }
        let cipher = self.email_token_cipher()?;
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        tx.query_one(
            "SELECT id FROM actors WHERE id=$1 AND active FOR UPDATE",
            &[&ctx.actor_id()],
        )
        .map_err(Self::database_error)?;
        tx.query_opt("SELECT id FROM sessions WHERE id=$1 AND actor_id=$2 AND revoked_at IS NULL AND expires_at>clock_timestamp() AND access_stage='ready' FOR SHARE",&[&ctx.credential_id(),&ctx.actor_id()]).map_err(Self::database_error)?.ok_or(PlatformError::Unauthorized)?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("email-address:{email}")],
        )
        .map_err(Self::database_error)?;
        let ready:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM installation_services WHERE kind='smtp' AND (settings->>'enabled')::boolean)",&[]).map_err(Self::database_error)?.get(0);
        if !ready {
            return Err(PlatformError::Conflict("email delivery is not configured"));
        }
        let current: String = tx
            .query_one(
                "SELECT COALESCE((SELECT email FROM notification_recipients WHERE actor_id=$1),'')",
                &[&ctx.actor_id()],
            )
            .map_err(Self::database_error)?
            .get(0);
        if current.eq_ignore_ascii_case(&email) {
            return Err(PlatformError::Validation("email address is unchanged"));
        }
        if tx.query_one("SELECT EXISTS(SELECT 1 FROM notification_recipients WHERE lower(email)=$1 AND actor_id<>$2)",&[&email,&ctx.actor_id()]).map_err(Self::database_error)?.get::<_,bool>(0){return Err(PlatformError::Conflict("email address unavailable"));}
        let count:i64=tx.query_one("SELECT count(*) FROM user_email_changes WHERE (actor_id=$1 OR email=$2) AND created_at>clock_timestamp()-interval '15 minutes'",&[&ctx.actor_id(),&email]).map_err(Self::database_error)?.get(0);
        if count >= 3 {
            return Err(PlatformError::Conflict(
                "wait before requesting another email code",
            ));
        }
        tx.execute("UPDATE user_email_changes SET used_at=clock_timestamp(),token_secret=NULL WHERE actor_id=$1 AND used_at IS NULL",&[&ctx.actor_id()]).map_err(Self::database_error)?;
        let id = SecretGenerator::id("emc")?;
        let code =
            zeroize::Zeroizing::new(crate::email_verification_code::email_verification_code()?);
        let sealed = cipher.seal(&id, 1, &code)?;
        let digest = SecretGenerator::digest(&format!("{id}:{}", code.as_str()));
        tx.execute("INSERT INTO user_email_changes(id,actor_id,email,previous_email,digest,token_secret,expires_at) VALUES($1,$2,$3,$4,$5,$6,clock_timestamp()+interval '15 minutes')",&[&id,&ctx.actor_id(),&email,&current,&digest,&sealed]).map_err(Self::database_error)?;
        tx.execute("INSERT INTO email_notifications(event_key,kind,resource_id,actor_id) VALUES($1,'email_verification',$2,$3)",&[&format!("email_verification:{id}"),&id,&ctx.actor_id()]).map_err(Self::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "user.email.requested",
            Some(&id),
            json!({}),
        )?;
        tx.commit().map_err(Self::database_error)
    }
    pub fn verify_own_email(&mut self, ctx: &AuthContext, code: &str) -> Result<(), PlatformError> {
        let ctx = self.fresh(ctx)?;
        if ctx.method() != AuthMethod::Session || ctx.kind() != ActorKind::Human {
            return Err(PlatformError::Forbidden);
        }
        if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
            return Err(PlatformError::Validation("invalid email verification code"));
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        tx.query_one(
            "SELECT id FROM actors WHERE id=$1 AND active FOR UPDATE",
            &[&ctx.actor_id()],
        )
        .map_err(Self::database_error)?;
        tx.query_opt("SELECT id FROM sessions WHERE id=$1 AND actor_id=$2 AND revoked_at IS NULL AND expires_at>clock_timestamp() AND access_stage='ready' FOR SHARE",&[&ctx.credential_id(),&ctx.actor_id()]).map_err(Self::database_error)?.ok_or(PlatformError::Unauthorized)?;
        let row=tx.query_opt("SELECT id,email,previous_email,digest,attempts FROM user_email_changes WHERE actor_id=$1 AND used_at IS NULL AND expires_at>clock_timestamp() ORDER BY created_at DESC LIMIT 1 FOR UPDATE",&[&ctx.actor_id()]).map_err(Self::database_error)?.ok_or(PlatformError::Validation("email verification expired"))?;
        let id: String = row.get(0);
        let email: String = row.get(1);
        let previous: String = row.get(2);
        let attempts: i32 = row.get(4);
        if attempts >= 5 {
            return Err(PlatformError::Conflict(
                "email verification attempts exhausted",
            ));
        }
        if !bool::from(
            row.get::<_, Vec<u8>>(3)
                .ct_eq(&SecretGenerator::digest(&format!("{id}:{code}"))),
        ) {
            tx.execute("UPDATE user_email_changes SET attempts=attempts+1,token_secret=CASE WHEN attempts>=4 THEN NULL ELSE token_secret END WHERE id=$1",&[&id]).map_err(Self::database_error)?;
            tx.commit().map_err(Self::database_error)?;
            return Err(PlatformError::Validation("invalid email verification code"));
        }
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("email-address:{email}")],
        )
        .map_err(Self::database_error)?;
        let current: String = tx
            .query_one(
                "SELECT COALESCE((SELECT email FROM notification_recipients WHERE actor_id=$1),'')",
                &[&ctx.actor_id()],
            )
            .map_err(Self::database_error)?
            .get(0);
        if current != previous {
            return Err(PlatformError::Conflict("email changed; request a new code"));
        }
        if tx.query_one("SELECT EXISTS(SELECT 1 FROM notification_recipients WHERE lower(email)=$1 AND actor_id<>$2)",&[&email,&ctx.actor_id()]).map_err(Self::database_error)?.get::<_,bool>(0){return Err(PlatformError::Conflict("email address unavailable"));}
        tx.execute("INSERT INTO notification_recipients(actor_id,email) VALUES($1,$2) ON CONFLICT(actor_id) DO UPDATE SET email=EXCLUDED.email,revision=notification_recipients.revision+1",&[&ctx.actor_id(),&email]).map_err(Self::database_error)?;
        tx.execute(
            "UPDATE user_email_changes SET used_at=clock_timestamp(),token_secret=NULL WHERE id=$1",
            &[&id],
        )
        .map_err(Self::database_error)?;
        tx.execute("UPDATE password_reset_requests SET used_at=clock_timestamp(),token_secret=NULL WHERE actor_id=$1 AND used_at IS NULL",&[&ctx.actor_id()]).map_err(Self::database_error)?;
        if !previous.is_empty() {
            tx.execute("INSERT INTO email_notifications(event_key,kind,resource_id,actor_id) VALUES($1,'email_changed',$2,$3)",&[&format!("email_changed:{id}"),&id,&ctx.actor_id()]).map_err(Self::database_error)?;
        }
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "user.email.verified",
            Some(&id),
            json!({}),
        )?;
        tx.commit().map_err(Self::database_error)
    }
}
