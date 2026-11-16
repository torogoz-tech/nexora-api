use crate::{EmailNotification, PlatformStore};
use nexora_platform::PlatformError;
use std::collections::BTreeMap;
impl PlatformStore {
    pub(crate) fn email_action_parameters(
        &mut self,
        mail: &EmailNotification,
        values: &mut BTreeMap<String, String>,
    ) -> Result<(), PlatformError> {
        match mail.kind.as_str() {
            "email_verification" => {
                let row=self.client.query_one("SELECT token_secret FROM user_email_changes WHERE id=$1 AND actor_id=$2 AND used_at IS NULL AND expires_at>clock_timestamp() AND attempts<5",&[&mail.resource_id,&mail.actor_id]).map_err(Self::database_error)?;
                let secret = self.email_token_cipher()?.open(
                    &mail.resource_id,
                    1,
                    row.get::<_, Option<serde_json::Value>>(0)
                        .ok_or(PlatformError::Storage)?,
                )?;
                values.insert("verification_code".into(), secret.as_str().to_owned());
            }
            "email_changed" => {
                let row=self.client.query_one("SELECT email FROM user_email_changes WHERE id=$1 AND actor_id=$2 AND used_at IS NOT NULL",&[&mail.resource_id,&mail.actor_id]).map_err(Self::database_error)?;
                values.insert("new_email".into(), row.get(0));
            }
            "invitation" => {
                let row=self.client.query_one("SELECT i.name,a.name,i.public_web_url,i.expires_at::text,i.token_secret,i.revision FROM member_invitations i JOIN actors a ON a.id=i.invited_by WHERE i.id=$1 AND i.status='pending' AND i.expires_at>clock_timestamp() AND $2='invitation:'||i.id||':'||i.revision",&[&mail.resource_id,&mail.event_key]).map_err(Self::database_error)?;
                let secret: serde_json::Value = row
                    .get::<_, Option<serde_json::Value>>(4)
                    .ok_or(PlatformError::Storage)?;
                let token =
                    self.email_token_cipher()?
                        .open(&mail.resource_id, row.get(5), secret)?;
                let web: String = row.get(2);
                values.insert("recipient_name".into(), row.get(0));
                values.insert("inviter_name".into(), row.get(1));
                values.insert("expires_at".into(), row.get(3));
                values.insert(
                    "invitation_url".into(),
                    format!("{web}/invitations/accept#token={}", token.as_str()),
                );
            }
            "password_reset" => {
                let row=self.client.query_one("SELECT public_web_url,expires_at::text,token_secret FROM password_reset_requests WHERE id=$1 AND used_at IS NULL AND expires_at>clock_timestamp()",&[&mail.resource_id]).map_err(Self::database_error)?;
                let token = self.email_token_cipher()?.open(
                    &mail.resource_id,
                    1,
                    row.get::<_, Option<serde_json::Value>>(2)
                        .ok_or(PlatformError::Storage)?,
                )?;
                let web: String = row.get(0);
                values.insert("expires_at".into(), row.get(1));
                values.insert(
                    "reset_url".into(),
                    format!("{web}/reset-password#token={}", token.as_str()),
                );
            }
            "invitation_accepted" => {
                let row=self.client.query_one("SELECT a.name FROM member_invitations i JOIN actors a ON a.id=i.accepted_by WHERE i.id=$1 AND i.status='accepted'",&[&mail.resource_id]).map_err(Self::database_error)?;
                values.insert("member_name".into(), row.get(0));
            }
            "approval_result" => {
                let row=self.client.query_one("SELECT status,COALESCE(decision_reason,'') FROM approval_requests WHERE id=$1 AND status IN ('approved','rejected')",&[&mail.resource_id]).map_err(Self::database_error)?;
                let status: String = row.get(0);
                let result = match (status.as_str(), mail.language.as_str()) {
                    ("approved", "es") => "Aprobada",
                    ("rejected", "es") => "Rechazada",
                    ("approved", _) => "Approved",
                    _ => "Rejected",
                };
                values.insert("decision".into(), result.into());
                values.insert("reason".into(), row.get(1));
            }
            _ => {}
        }
        Ok(())
    }
}
