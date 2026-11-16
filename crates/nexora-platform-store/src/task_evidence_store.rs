use crate::{
    PlatformStore, SecretGenerator, audit_writer::AuditWriter, receipt_writer::ReceiptWriter,
    task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::{AuthContext, Capability, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn upload_task_evidence(
        &mut self,
        ctx: &AuthContext,
        name: &str,
        bytes: &[u8],
    ) -> Result<Value, PlatformError> {
        if name.trim().is_empty()
            || name.len() > 200
            || name.chars().any(char::is_control)
            || bytes.is_empty()
            || bytes.len() > 8388608
            || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        {
            return Err(PlatformError::Validation("invalid task evidence image"));
        }
        let request = self.request_context.clone();
        let namespace = self.namespace.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        TaskScopeAuthority::require(&mut tx, ctx, Capability::TasksWrite, None)?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("task-evidence-upload:{}", ctx.actor_id())],
        )
        .map_err(Self::database_error)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.evidence.upload",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        tx.execute("DELETE FROM task_evidence WHERE uploaded_by=$1 AND task_id IS NULL AND created_at<clock_timestamp()-interval '24 hours'",&[&ctx.actor_id()]).map_err(Self::database_error)?;
        let count: i64 = tx
            .query_one(
                "SELECT count(*) FROM task_evidence WHERE uploaded_by=$1 AND task_id IS NULL",
                &[&ctx.actor_id()],
            )
            .map_err(Self::database_error)?
            .get(0);
        if count >= 32 {
            return Err(PlatformError::Validation(
                "too many pending evidence images",
            ));
        }
        let id = SecretGenerator::id("tev")?;
        let sha = SecretGenerator::checksum(bytes);
        tx.execute(
            "INSERT INTO task_evidence(id,uploaded_by,name,payload,sha256) VALUES($1,$2,$3,$4,$5)",
            &[&id, &ctx.actor_id(), &name, &bytes, &sha],
        )
        .map_err(Self::database_error)?;
        let value = json!({"id":id,"url":format!("/api/v1/task-evidence/{id}/content"),"name":name,"size_bytes":bytes.len(),"sha256":sha});
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "task.evidence.uploaded",
            Some(&id),
            json!({"sha256":sha,"size_bytes":bytes.len()}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn task_evidence_content(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Vec<u8>, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        let row=tx.query_opt("SELECT task_id,uploaded_by,payload,sha256,created_at>clock_timestamp()-interval '24 hours' FROM task_evidence WHERE id=$1",&[&id]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        if let Some(task) = row.get::<_, Option<String>>(0) {
            TaskScopeAuthority::task(&mut tx, ctx, &task, Capability::TasksRead)?;
        } else {
            TaskScopeAuthority::require(&mut tx, ctx, Capability::TasksWrite, None)?;
            if row.get::<_, String>(1) != ctx.actor_id() || !row.get::<_, bool>(4) {
                return Err(PlatformError::NotFound);
            }
        }
        let bytes: Vec<u8> = row.get(2);
        if SecretGenerator::checksum(&bytes) != row.get::<_, String>(3) {
            return Err(PlatformError::Storage);
        }
        tx.commit().map_err(Self::database_error)?;
        Ok(bytes)
    }
}
