use crate::{PlatformStore, SecretGenerator, audit_writer::AuditWriter};
use nexora_platform::PlatformError;
use serde_json::{Value, json};
use subtle::ConstantTimeEq;
impl PlatformStore {
    /// Non-secret browser metadata; restricted challenges cannot become ordinary API sessions.
    pub fn browser_session(&mut self, token: &str) -> Result<Value, PlatformError> {
        if token.len() > 256 {
            return Err(PlatformError::Unauthorized);
        }
        let (id, _) = token
            .strip_prefix("nxs_")
            .and_then(|v| v.split_once('.'))
            .ok_or(PlatformError::Unauthorized)?;
        let row = self.client.query_opt("SELECT s.actor_id,a.name,s.access_stage,ceil(extract(epoch FROM s.expires_at-now()))::bigint,s.digest FROM sessions s JOIN actors a ON a.id=s.actor_id JOIN users u ON u.actor_id=a.id WHERE s.id=$1 AND s.revoked_at IS NULL AND s.expires_at>now() AND a.active AND (s.access_stage='ready' OR s.created_at>now()-interval '15 minutes') AND (s.authentication_provider_id IS NULL OR EXISTS(SELECT 1 FROM oidc_providers p WHERE p.id=s.authentication_provider_id AND p.enabled))", &[&id]).map_err(Self::database_error)?.ok_or(PlatformError::Unauthorized)?;
        if !bool::from(
            row.get::<_, Vec<u8>>(4)
                .ct_eq(&SecretGenerator::digest(token)),
        ) {
            return Err(PlatformError::Unauthorized);
        }
        let stage: String = row.get(2);
        Ok(
            json!({"browser_session":true,"actor_id":row.get::<_,String>(0),"display_name":row.get::<_,String>(1),"session_id":id,"expires_in":row.get::<_,i64>(3),"next_step":(stage!="ready").then_some(stage)}),
        )
    }
    pub fn end_browser_session(&mut self, token: &str) -> Result<(), PlatformError> {
        let metadata = self.browser_session(token)?;
        let id = metadata["session_id"]
            .as_str()
            .ok_or(PlatformError::Unauthorized)?;
        let actor = metadata["actor_id"]
            .as_str()
            .ok_or(PlatformError::Unauthorized)?;
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        tx.execute(
            "UPDATE sessions SET revoked_at=now() WHERE id=$1 AND digest=$2 AND revoked_at IS NULL",
            &[&id, &SecretGenerator::digest(token)],
        )
        .map_err(Self::database_error)?;
        AuditWriter::append(
            &mut tx,
            Some(actor),
            Some(id),
            "session.revoked",
            Some(id),
            json!({"transport":"cookie"}),
        )?;
        tx.commit().map_err(Self::database_error)
    }
}
