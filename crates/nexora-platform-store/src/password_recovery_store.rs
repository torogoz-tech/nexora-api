use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    password_credential::PasswordCredential,
};
use nexora_platform::PlatformError;
use serde_json::json;
use zeroize::Zeroizing;
impl PlatformStore {
    /// Enumeration-safe: no result reveals whether an account exists or a message was queued.
    pub fn request_password_recovery(&mut self, email: &str) -> Result<(), PlatformError> {
        let email = email.trim().to_ascii_lowercase();
        if email.len() > 320
            || !email_address::EmailAddress::is_valid(&email)
            || email.chars().any(|c| c.is_control() || c.is_whitespace())
        {
            return Ok(());
        }
        let cipher = self.email_token_cipher()?;
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended('nexora-password-reset-rate',0))",
            &[],
        )
        .map_err(Self::database_error)?;
        tx.execute(
            "DELETE FROM email_action_requests WHERE created_at<clock_timestamp()-interval '1 day'",
            &[],
        )
        .map_err(Self::database_error)?;
        let digest = SecretGenerator::digest(&email);
        let counts=tx.query_one("SELECT count(*),count(*) FILTER(WHERE address_digest=$1) FROM email_action_requests WHERE created_at>clock_timestamp()-interval '15 minutes'",&[&digest]).map_err(Self::database_error)?;
        if counts.get::<_, i64>(0) >= 100 || counts.get::<_, i64>(1) >= 3 {
            tx.commit().map_err(Self::database_error)?;
            return Ok(());
        }
        tx.execute(
            "INSERT INTO email_action_requests(address_digest) VALUES($1)",
            &[&digest],
        )
        .map_err(Self::database_error)?;
        let web: String = tx
            .query_one(
                "SELECT public_web_url FROM email_access_settings WHERE singleton",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        let ready:bool=tx.query_one("SELECT (SELECT local_enabled FROM authentication_policy WHERE singleton) AND EXISTS(SELECT 1 FROM installation_services WHERE kind='smtp' AND (settings->>'enabled')::boolean)",&[]).map_err(Self::database_error)?.get(0);
        let accounts=tx.query("SELECT a.id FROM actors a JOIN users u ON u.actor_id=a.id JOIN notification_recipients r ON r.actor_id=a.id WHERE lower(r.email)=$1 AND a.active AND a.kind='human' AND u.password_hash IS NOT NULL",&[&email]).map_err(Self::database_error)?;
        if web.is_empty() || !ready || accounts.len() != 1 {
            tx.commit().map_err(Self::database_error)?;
            return Ok(());
        }
        let actor: String = accounts[0].get(0);
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("password-reset:{actor}")],
        )
        .map_err(Self::database_error)?;
        tx.execute("UPDATE password_reset_requests SET used_at=clock_timestamp(),token_secret=NULL WHERE actor_id=$1 AND used_at IS NULL",&[&actor]).map_err(Self::database_error)?;
        let id = SecretGenerator::id("rst")?;
        let token = Zeroizing::new(SecretGenerator::token("nxr")?);
        let secret = cipher.seal(&id, 1, &token)?;
        tx.execute("INSERT INTO password_reset_requests(id,actor_id,email,digest,token_secret,public_web_url,expires_at) VALUES($1,$2,$3,$4,$5,$6,clock_timestamp()+interval '30 minutes')",&[&id,&actor,&email,&SecretGenerator::digest(&token),&secret,&web]).map_err(Self::database_error)?;
        tx.execute("INSERT INTO email_notifications(event_key,kind,resource_id,actor_id) VALUES($1,'password_reset',$2,$3)",&[&format!("password_reset:{id}"),&id,&actor]).map_err(Self::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(&actor),
            None,
            "password.recovery.requested",
            Some(&id),
            json!({}),
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(())
    }
    pub fn complete_password_recovery(
        &mut self,
        token: &str,
        password: &str,
    ) -> Result<(), PlatformError> {
        if !Self::valid_email_action_token(token, "nxr_") {
            return Err(PlatformError::Validation(
                "invalid or expired recovery link",
            ));
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let actor = tx
            .query_opt(
                "SELECT actor_id FROM password_reset_requests WHERE digest=$1",
                &[&SecretGenerator::digest(token)],
            )
            .map_err(Self::database_error)?
            .map(|r| r.get::<_, String>(0))
            .ok_or(PlatformError::Validation(
                "invalid or expired recovery link",
            ))?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("password-reset:{actor}")],
        )
        .map_err(Self::database_error)?;
        let row=tx.query_opt("SELECT q.id FROM password_reset_requests q JOIN actors a ON a.id=q.actor_id JOIN users u ON u.actor_id=a.id JOIN notification_recipients r ON r.actor_id=a.id WHERE q.digest=$1 AND q.used_at IS NULL AND q.expires_at>clock_timestamp() AND a.active AND a.kind='human' AND u.password_hash IS NOT NULL AND lower(r.email)=q.email AND q.public_web_url=(SELECT public_web_url FROM email_access_settings WHERE singleton) AND (SELECT local_enabled FROM authentication_policy WHERE singleton) FOR UPDATE OF q,a,u,r",&[&SecretGenerator::digest(token)]).map_err(Self::database_error)?.ok_or(PlatformError::Validation("invalid or expired recovery link"))?;
        let id: String = row.get(0);
        let local_enabled: bool = tx
            .query_one(
                "SELECT local_enabled FROM authentication_policy WHERE singleton FOR SHARE",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        if !local_enabled {
            return Err(PlatformError::Validation(
                "invalid or expired recovery link",
            ));
        }
        let hash = PasswordCredential::hash(password)?;
        tx.execute(
            "UPDATE users SET password_hash=$2 WHERE actor_id=$1",
            &[&actor, &hash],
        )
        .map_err(Self::database_error)?;
        tx.execute("UPDATE sessions SET revoked_at=clock_timestamp() WHERE actor_id=$1 AND revoked_at IS NULL",&[&actor]).map_err(Self::database_error)?;
        tx.execute("UPDATE password_reset_requests SET used_at=clock_timestamp(),token_secret=NULL WHERE actor_id=$1 AND used_at IS NULL",&[&actor]).map_err(Self::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(&actor),
            None,
            "password.recovery.completed",
            Some(&id),
            json!({}),
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(())
    }
}
