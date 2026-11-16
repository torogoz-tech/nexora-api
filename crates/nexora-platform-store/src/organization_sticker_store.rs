use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter,
    platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
};
use nexora_platform::{AuthContext, Capability, DomainText, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn organization_stickers(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::OrganizationKnowledgeRead, None)?;
        let items:Vec<Value>=tx.query("SELECT jsonb_build_object('id',id,'name',name,'url','/api/v1/stickers/'||id||'/content') FROM organization_stickers WHERE NOT archived ORDER BY lower(name),id",&[]).map_err(Self::database_error)?.into_iter().map(|row|row.get(0)).collect();
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"items":items}))
    }
    pub fn upload_organization_sticker(
        &mut self,
        ctx: &AuthContext,
        name: &str,
        bytes: &[u8],
    ) -> Result<Value, PlatformError> {
        let name = DomainText::required(name, 80)?;
        if bytes.is_empty() || bytes.len() > 8388608 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(PlatformError::Validation("invalid sticker image"));
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("{}:stickers", self.namespace)],
        )
        .map_err(Self::database_error)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "sticker.upload",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        let count: i64 = tx
            .query_one(
                "SELECT count(*) FROM organization_stickers WHERE NOT archived",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        if count >= 200 {
            return Err(PlatformError::Validation("sticker catalog is full"));
        }
        let id = SecretGenerator::id("stk")?;
        let sha = SecretGenerator::checksum(bytes);
        tx.execute("INSERT INTO organization_stickers(id,name,payload,sha256,created_by) VALUES($1,$2,$3,$4,$5)",&[&id,&name,&bytes,&sha,&ctx.actor_id()]).map_err(Self::database_error)?;
        let value = json!({"id":id,"name":name,"url":format!("/api/v1/stickers/{id}/content")});
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "sticker.uploaded",
            Some(&id),
            json!({"sha256":sha}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn archive_organization_sticker(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Value, PlatformError> {
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::InstallationManage, None)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "sticker.archive",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        if tx
            .execute(
                "UPDATE organization_stickers SET archived=true WHERE id=$1",
                &[&id],
            )
            .map_err(Self::database_error)?
            == 0
        {
            return Err(PlatformError::NotFound);
        }
        let value = json!({"id":id,"archived":true});
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "sticker.archived",
            Some(id),
            json!({}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn organization_sticker_content(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Vec<u8>, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::OrganizationKnowledgeRead, None)?;
        // Archived stickers remain readable in existing comments.
        let row = tx
            .query_opt(
                "SELECT payload,sha256 FROM organization_stickers WHERE id=$1",
                &[&id],
            )
            .map_err(Self::database_error)?
            .ok_or(PlatformError::NotFound)?;
        let bytes: Vec<u8> = row.get(0);
        if SecretGenerator::checksum(&bytes) != row.get::<_, String>(1) {
            return Err(PlatformError::Storage);
        }
        tx.commit().map_err(Self::database_error)?;
        Ok(bytes)
    }
}
