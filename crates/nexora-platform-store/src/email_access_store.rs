use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::json;
impl PlatformStore {
    pub fn email_access_settings(
        &mut self,
        ctx: &AuthContext,
    ) -> Result<EmailAccessSettings, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        let row = tx
            .query_one(
                "SELECT public_web_url,revision FROM email_access_settings WHERE singleton",
                &[],
            )
            .map_err(Self::database_error)?;
        let value = EmailAccessSettings {
            public_web_url: row.get(0),
            revision: row.get(1),
        };
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn change_email_access_settings(
        &mut self,
        ctx: &AuthContext,
        input: &EmailAccessChange,
    ) -> Result<EmailAccessSettings, PlatformError> {
        let url = EmailAccessSettings::validate(&input.public_web_url)?;
        let receipt = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "email.access.change",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Self::decode(value);
        }
        let row=tx.query_opt("UPDATE email_access_settings SET public_web_url=$1,revision=revision+1 WHERE singleton AND revision=$2 RETURNING revision",&[&url,&input.expected_revision]).map_err(Self::database_error)?.ok_or(PlatformError::Conflict("email access settings changed"))?;
        let value = EmailAccessSettings {
            public_web_url: url,
            revision: row.get(0),
        };
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "email.access.change",
            None,
            json!({"revision":value.revision}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &json!(value))?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn organization_members(
        &mut self,
        ctx: &AuthContext,
    ) -> Result<serde_json::Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::MembersRead, None).or_else(
            |error| match error {
                PlatformError::Forbidden => {
                    PlatformAuthority::require(&mut tx, ctx, Capability::ActorsManage, None)
                }
                error => Err(error),
            },
        )?;
        let rows = tx
            .query(
                &format!(
                    "SELECT {} FROM actors a JOIN users u ON u.actor_id=a.id
              LEFT JOIN notification_recipients r ON r.actor_id=a.id
              WHERE a.kind='human' AND NOT EXISTS(SELECT 1 FROM actor_roles ar WHERE ar.actor_id=a.id AND ar.role='super_admin') ORDER BY a.name,a.id LIMIT 1000",
                    crate::member_public_projection::member_public_projection()
                ),
                &[],
            )
            .map_err(Self::database_error)?;
        let values: Vec<serde_json::Value> = rows.iter().map(|r| r.get(0)).collect();
        tx.commit().map_err(Self::database_error)?;
        Ok(json!(values))
    }
}
