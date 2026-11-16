use crate::{
    PlatformStore, SecretGenerator, receipt_writer::ReceiptWriter,
    task_history_writer::TaskHistoryWriter, task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::{AuthContext, Capability, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn upload_task_image(
        &mut self,
        ctx: &AuthContext,
        task_id: &str,
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
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        tx.query_opt("SELECT id FROM tasks WHERE id=$1 FOR UPDATE", &[&task_id])
            .map_err(Self::database_error)?
            .ok_or(PlatformError::NotFound)?;
        let task = TaskScopeAuthority::task(&mut tx, ctx, task_id, Capability::TasksWrite)?;
        if task.archived {
            return Err(PlatformError::Conflict("task is archived"));
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.image.upload",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        let count: i64 = tx
            .query_one(
                "SELECT count(*) FROM task_evidence WHERE task_id=$1",
                &[&task_id],
            )
            .map_err(Self::database_error)?
            .get(0);
        if count >= 100 {
            return Err(PlatformError::Validation("task image limit reached"));
        }
        let id = SecretGenerator::id("tev")?;
        let sha = SecretGenerator::checksum(bytes);
        tx.execute("INSERT INTO task_evidence(id,task_id,uploaded_by,name,payload,sha256) VALUES($1,$2,$3,$4,$5,$6)",&[&id,&task_id,&ctx.actor_id(),&name,&bytes,&sha]).map_err(Self::database_error)?;
        let url = format!("/api/v1/task-evidence/{id}/content");
        let value = json!({"id":id,"task_id":task_id,"name":name,"url":url,"markdown":format!("![Evidence]({url})"),"size_bytes":bytes.len(),"sha256":sha});
        TaskHistoryWriter::event(
            &mut tx,
            ctx,
            &task,
            "task.image_added",
            json!({"image":value}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
