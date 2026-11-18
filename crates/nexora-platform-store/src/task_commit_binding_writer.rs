use crate::{
    PlatformStore, SecretGenerator, receipt_writer::ReceiptWriter,
    task_history_writer::TaskHistoryWriter, task_policy_store::TaskPolicyStore,
    task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_verified_task_commit(
        &mut self,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        revision: i64,
        repository: &str,
        commit: &str,
        evidence: &Value,
    ) -> Result<Value, PlatformError> {
        if evidence["provenance"] != "provider_verified" {
            return Err(PlatformError::Validation(
                "task commit binding requires provider verification",
            ));
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        TaskScopeAuthority::require(&mut tx, ctx, Capability::TasksWrite, Some(project))?;
        TaskScopeAuthority::modify(&mut tx, ctx, Some(project), id, false)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.commit_bind",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        let before = TaskPolicyStore::scoped_task(&mut tx, Some(project), id)?;
        if before.revision != revision || before.archived {
            return Err(PlatformError::Conflict(
                "task changed during provider commit verification",
            ));
        }
        let current =
            crate::task_transition_provider_authority::TaskTransitionProviderAuthority::lock(
                &mut tx, ctx, project, repository,
            )?;
        if current != evidence["authority"] {
            return Err(PlatformError::Conflict(
                "repository authority changed during commit binding",
            ));
        }
        if tx.query_one("SELECT EXISTS(SELECT 1 FROM task_commit_links WHERE task_id=$1 AND repository_id=$2 AND commit_sha=$3 AND removed_at IS NULL)",&[&id,&repository,&commit]).map_err(Self::database_error)?.get::<_,bool>(0){return Err(PlatformError::Conflict("commit is already linked to this task"));}
        let link = SecretGenerator::id("tcl")?;
        let value:Value=tx.query_one("INSERT INTO task_commit_links AS l(id,task_id,repository_id,commit_sha,sequence,provider_evidence,actor_id) VALUES($1,$2,$3,$4,COALESCE((SELECT max(sequence)+1 FROM task_commit_links WHERE task_id=$2),1),$5,$6) ON CONFLICT(task_id,repository_id,commit_sha) DO UPDATE SET removed_at=NULL,provider_evidence=EXCLUDED.provider_evidence,actor_id=EXCLUDED.actor_id,sequence=EXCLUDED.sequence RETURNING to_jsonb(l)-'provider_evidence'",&[&link,&id,&repository,&commit,&evidence,&ctx.actor_id()]).map_err(Self::database_error)?.get(0);
        let after:WorkTask=Self::decode(tx.query_one("UPDATE tasks AS t SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(t)",&[&id]).map_err(Self::database_error)?.get(0))?;
        TaskHistoryWriter::event(
            &mut tx,
            ctx,
            &after,
            "task.commit_bound",
            json!({"before":TaskHistoryWriter::state(&before)?,"after":TaskHistoryWriter::state(&after)?,"commit":value}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
#[cfg(test)]
#[path = "task_commit_binding_writer_tests.rs"]
mod tests;
