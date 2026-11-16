use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::json;
impl PlatformStore {
    pub fn email_template(&mut self, ctx: &AuthContext) -> Result<EmailTemplate, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        let row = tx
            .query_one(
                "SELECT html, revision FROM installation_email_template WHERE singleton",
                &[],
            )
            .map_err(Self::database_error)?;
        let template = EmailTemplate {
            html: row
                .get::<_, Option<String>>(0)
                .unwrap_or_else(|| EmailTemplate::DEFAULT_HTML.to_owned()),
            revision: row.get(1),
        };
        tx.commit().map_err(Self::database_error)?;
        Ok(template)
    }
    pub fn email_template_for_delivery(
        &mut self,
    ) -> Result<(EmailTemplate, String), PlatformError> {
        let row = self.client.query_one("SELECT t.html,t.revision,p.name FROM installation_email_template t CROSS JOIN organization_profile p WHERE t.singleton AND p.singleton", &[]).map_err(Self::database_error)?;
        Ok((
            EmailTemplate {
                html: row
                    .get::<_, Option<String>>(0)
                    .unwrap_or_else(|| EmailTemplate::DEFAULT_HTML.to_owned()),
                revision: row.get(1),
            },
            row.get(2),
        ))
    }
    pub fn change_email_template(
        &mut self,
        ctx: &AuthContext,
        input: &EmailTemplateChange,
    ) -> Result<EmailTemplate, PlatformError> {
        EmailTemplate::validate(&input.html)?;
        let receipt = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "email.template.change",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Self::decode(value);
        }
        let row = tx.query_opt("UPDATE installation_email_template SET html=$1, revision=revision+1 WHERE singleton AND revision=$2 RETURNING revision", &[&input.html, &input.expected_revision]).map_err(Self::database_error)?.ok_or(PlatformError::Conflict("email template changed"))?;
        let template = EmailTemplate {
            html: input.html.clone(),
            revision: row.get(0),
        };
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "email.template.change",
            None,
            json!({"revision":template.revision}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &json!(template))?;
        tx.commit().map_err(Self::database_error)?;
        Ok(template)
    }
}
