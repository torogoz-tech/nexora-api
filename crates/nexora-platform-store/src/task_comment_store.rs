use crate::{
    PlatformStore, SecretGenerator, receipt_writer::ReceiptWriter,
    task_history_writer::TaskHistoryWriter, task_policy_store::TaskPolicyStore,
};
use nexora_platform::*;
use serde_json::{Value, json};

pub(crate) struct TaskCommentStore;
impl TaskCommentStore {
    pub fn add(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        task: &str,
        input: &TaskCommentCreate,
    ) -> Result<TaskComment, PlatformError> {
        let body = DomainText::required(&input.body, 8000)?;
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        crate::task_scope_authority::TaskScopeAuthority::require(
            &mut tx,
            ctx,
            Capability::TasksWrite,
            project,
        )?;
        crate::task_scope_authority::TaskScopeAuthority::relation(&mut tx, ctx, project, task)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.comment_add",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let resource = TaskPolicyStore::scoped_task(&mut tx, project, task)?;
        if resource.archived {
            return Err(PlatformError::Conflict("task is archived"));
        }
        crate::task_evidence_binding::TaskEvidenceBinding::claim(&mut tx, ctx, task, &body)?;
        let sequence:i64=tx.query_one("UPDATE tasks SET comment_sequence=comment_sequence+1 WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2 RETURNING comment_sequence",&[&task,&project]).map_err(PlatformStore::database_error)?.get(0);
        let id = SecretGenerator::id("tcm")?;
        let value:Value=tx.query_one("INSERT INTO task_comments AS c(id,project_id,task_id,sequence,body,actor_id,credential_id) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING to_jsonb(c)",&[&id,&project,&task,&sequence,&body,&ctx.actor_id(),&ctx.credential_id()]).map_err(PlatformStore::database_error)?.get(0);
        TaskHistoryWriter::event(
            &mut tx,
            ctx,
            &resource,
            "task.comment_added",
            json!({"comment":value}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
