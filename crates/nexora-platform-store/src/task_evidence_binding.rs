use crate::{PlatformStore, audit_writer::AuditWriter};
use nexora_platform::{AuthContext, PlatformError};
use postgres::Transaction;
use std::collections::BTreeSet;
pub(crate) struct TaskEvidenceBinding;
impl TaskEvidenceBinding {
    pub fn claim(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        task: &str,
        context: &str,
    ) -> Result<(), PlatformError> {
        let mut ids = BTreeSet::new();
        for tail in context.split("/api/v1/task-evidence/").skip(1) {
            let id: String = tail
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !id.starts_with("tev_")
                || id.len() > 128
                || !tail[id.len()..].starts_with("/content")
            {
                return Err(PlatformError::Validation("invalid task evidence reference"));
            }
            ids.insert(id);
        }
        if ids.len() > 32 {
            return Err(PlatformError::Validation("too many task evidence images"));
        }
        for id in ids {
            let row=tx.query_opt("SELECT task_id,uploaded_by,created_at>clock_timestamp()-interval '24 hours' FROM task_evidence WHERE id=$1 FOR UPDATE",&[&id]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Validation("task evidence unavailable"))?;
            let bound: Option<String> = row.get(0);
            if bound.as_deref() == Some(task) {
                continue;
            }
            if bound.is_some()
                || row.get::<_, String>(1) != ctx.actor_id()
                || !row.get::<_, bool>(2)
            {
                return Err(PlatformError::Validation(
                    "task evidence belongs to another task or draft",
                ));
            }
            tx.execute(
                "UPDATE task_evidence SET task_id=$2 WHERE id=$1",
                &[&id, &task],
            )
            .map_err(PlatformStore::database_error)?;
            AuditWriter::append(
                tx,
                Some(ctx.actor_id()),
                Some(ctx.credential_id()),
                "task.evidence.attached",
                Some(task),
                serde_json::json!({"evidence_id":id}),
            )?;
        }
        Ok(())
    }
}
