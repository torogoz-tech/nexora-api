use crate::{PlatformStore, audit_writer::AuditWriter};
use nexora_platform::PlatformError;
use postgres::Transaction;
use serde_json::json;
pub(crate) struct LoginGate;
impl LoginGate {
    pub fn apply(
        tx: &mut Transaction<'_>,
        actor: &str,
        session: &str,
    ) -> Result<Option<String>, PlatformError> {
        tx.execute(
            "INSERT INTO user_security(actor_id) VALUES($1) ON CONFLICT DO NOTHING",
            &[&actor],
        )
        .map_err(PlatformStore::database_error)?;
        let row=tx.query_one("SELECT pending_role,totp_exempt,secret IS NOT NULL,EXISTS(SELECT 1 FROM actor_roles WHERE actor_id=$1),primary_auth_method FROM user_security WHERE actor_id=$1 FOR UPDATE", &[&actor]).map_err(PlatformStore::database_error)?;
        let primary_method: String = row.get(4);
        let stage = if row.get::<_, bool>(0) && !row.get::<_, bool>(3) {
            "pending_role"
        } else if row.get::<_, bool>(1) || primary_method == "passkey" {
            "ready"
        } else if primary_method == "totp" && row.get::<_, bool>(2) {
            "totp_verify"
        } else if primary_method == "none" && !row.get::<_, bool>(2) {
            "totp_enroll"
        } else {
            "ready"
        };
        tx.execute("UPDATE sessions SET access_stage=$2,expires_at=CASE WHEN $2='ready' THEN expires_at ELSE now()+interval '15 minutes' END WHERE id=$1", &[&session,&stage]).map_err(PlatformStore::database_error)?;
        AuditWriter::append(
            tx,
            Some(actor),
            Some(session),
            "identity.login_gate",
            Some(actor),
            json!({"stage":stage}),
        )?;
        Ok((stage != "ready").then(|| stage.to_owned()))
    }
}
