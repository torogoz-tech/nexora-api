use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    knowledge_contribution_policy::KnowledgeContributionPolicy, receipt_writer::ReceiptWriter,
};
use nexora_platform::{AuthContext, DomainText, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn upload_knowledge_image(
        &mut self,
        ctx: &AuthContext,
        project: Option<&str>,
        name: &str,
        bytes: &[u8],
    ) -> Result<Value, PlatformError> {
        let name = DomainText::required(name, 200)?;
        if bytes.is_empty() || bytes.len() > 8388608 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(PlatformError::Validation("invalid knowledge image"));
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        KnowledgeContributionPolicy::write(&mut tx, ctx, project, false)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "knowledge.image.upload",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        let id = SecretGenerator::id("kim")?;
        tx.execute("INSERT INTO knowledge_images(id,project_id,uploaded_by,name,payload) VALUES($1,$2,$3,$4,$5)", &[&id,&project,&ctx.actor_id(),&name,&bytes]).map_err(Self::database_error)?;
        let value = json!({"id":id,"url":format!("/api/v1/knowledge/images/{id}/content")});
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "knowledge.image.upload",
            Some(&id),
            json!({"project_id":project}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn knowledge_image_content(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Vec<u8>, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let row = tx
            .query_opt(
                "SELECT project_id,payload FROM knowledge_images WHERE id=$1",
                &[&id],
            )
            .map_err(Self::database_error)?
            .ok_or(PlatformError::NotFound)?;
        let project: Option<String> = row.get(0);
        KnowledgeContributionPolicy::read(&mut tx, ctx, project.as_deref())?;
        let bytes = row.get(1);
        tx.commit().map_err(Self::database_error)?;
        Ok(bytes)
    }
}
