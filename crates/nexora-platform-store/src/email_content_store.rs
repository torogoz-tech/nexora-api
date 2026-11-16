use crate::{
    EmailNotification, PlatformStore, audit_writer::AuditWriter,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::json;
use std::collections::BTreeMap;
impl PlatformStore {
    pub fn email_contents(
        &mut self,
        ctx: &AuthContext,
    ) -> Result<Vec<EmailContentTemplate>, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        let rows=tx.query("SELECT kind,language,subject,body_html,revision FROM installation_email_contents ORDER BY kind,language",&[]).map_err(Self::database_error)?;
        let mut result = Vec::new();
        for row in rows {
            let mut item = EmailContentTemplate::definition(row.get(0), row.get(1))?;
            item.subject = row.get(2);
            item.body_html = row.get(3);
            item.revision = row.get(4);
            item.customized =
                item.subject != item.default_subject || item.body_html != item.default_body_html;
            result.push(item);
        }
        tx.commit().map_err(Self::database_error)?;
        Ok(result)
    }
    pub fn email_content(
        &mut self,
        ctx: &AuthContext,
        kind: &str,
        language: &str,
    ) -> Result<EmailContentTemplate, PlatformError> {
        self.email_contents(ctx)?
            .into_iter()
            .find(|v| v.kind == kind && v.language == language)
            .ok_or(PlatformError::NotFound)
    }
    pub fn change_email_content(
        &mut self,
        ctx: &AuthContext,
        kind: &str,
        language: &str,
        input: &EmailContentChange,
    ) -> Result<EmailContentTemplate, PlatformError> {
        let mut item = EmailContentTemplate::definition(kind, language)?;
        item.validate(&input.subject, &input.body_html)?;
        let receipt = self.request_context.clone();
        let operation = "email.content.change";
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            operation,
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Self::decode(value);
        }
        let row=tx.query_opt("UPDATE installation_email_contents SET subject=$1,body_html=$2,revision=revision+1 WHERE kind=$3 AND language=$4 AND revision=$5 RETURNING revision",&[&input.subject,&input.body_html,&kind,&language,&input.expected_revision]).map_err(Self::database_error)?.ok_or(PlatformError::Conflict("email content changed"))?;
        item.subject = input.subject.clone();
        item.body_html = input.body_html.clone();
        item.revision = row.get(0);
        item.customized =
            item.subject != item.default_subject || item.body_html != item.default_body_html;
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            operation,
            None,
            json!({"kind":kind,"language":language,"revision":item.revision}),
        )?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &json!(item))?;
        tx.commit().map_err(Self::database_error)?;
        Ok(item)
    }
    /// Called only after delivery authority and current recipient preferences were checked.
    pub fn email_content_for_delivery(
        &mut self,
        mail: &EmailNotification,
        organization: &str,
    ) -> Result<(String, String), PlatformError> {
        let row=self.client.query_one("SELECT subject,body_html FROM installation_email_contents WHERE kind=$1 AND language=$2",&[&mail.kind,&mail.language]).map_err(Self::database_error)?;
        let mut item = EmailContentTemplate::definition(&mail.kind, &mail.language)?;
        item.subject = row.get(0);
        item.body_html = row.get(1);
        let name: String = self
            .client
            .query_one("SELECT name FROM actors WHERE id=$1", &[&mail.actor_id])
            .map_err(Self::database_error)?
            .get(0);
        let mut values = BTreeMap::from([
            ("organization_name".into(), organization.into()),
            ("recipient_name".into(), name),
        ]);
        if let Some(project) = &mail.project_id {
            let name: String = self
                .client
                .query_one("SELECT name FROM projects WHERE id=$1", &[&project])
                .map_err(Self::database_error)?
                .get(0);
            values.insert("project_name".into(), name);
        }
        values.insert("resource_id".into(), mail.resource_id.clone());
        self.email_action_parameters(mail, &mut values)?;
        item.render(&values)
    }
}
