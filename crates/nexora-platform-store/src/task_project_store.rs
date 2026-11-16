use crate::{
    PlatformStore, receipt_writer::ReceiptWriter, task_history_writer::TaskHistoryWriter,
    task_policy_store::TaskPolicyStore, task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::*;
use serde_json::Value;
impl PlatformStore {
    pub fn change_task_project(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &TaskProjectChange,
    ) -> Result<WorkTask, PlatformError> {
        if input.expected_revision < 1
            || input
                .project_id
                .as_ref()
                .is_some_and(|p| p.is_empty() || p.len() > 128)
        {
            return Err(PlatformError::Validation("invalid task project change"));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let request = self.request_context.clone();
        let namespace = self.namespace.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        crate::task_relation_lock::TaskRelationLock::acquire(&mut tx)?;
        tx.query_opt("SELECT id FROM tasks WHERE id=$1 FOR UPDATE", &[&id])
            .map_err(Self::database_error)?
            .ok_or(PlatformError::NotFound)?;
        let before = TaskScopeAuthority::task(&mut tx, ctx, id, Capability::TasksRead)?;
        TaskScopeAuthority::require(
            &mut tx,
            ctx,
            Capability::TasksWrite,
            before.project_id.as_deref(),
        )?;
        TaskScopeAuthority::modify(&mut tx, ctx, before.project_id.as_deref(), id, false)?;
        for cap in [Capability::TasksRead, Capability::TasksWrite] {
            TaskScopeAuthority::require(&mut tx, ctx, cap, input.project_id.as_deref())?;
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.project.change",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Self::decode(value);
        }
        if before.revision != input.expected_revision || before.archived {
            return Err(PlatformError::Conflict("task revision changed"));
        }
        if before.project_id == input.project_id {
            return Err(PlatformError::Validation(
                "task already belongs to this project",
            ));
        }
        let anchored:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM task_subtasks WHERE parent_task_id=$1 OR child_task_id=$1) OR EXISTS(SELECT 1 FROM task_dependencies WHERE source_task_id=$1 OR target_task_id=$1) OR EXISTS(SELECT 1 FROM document_links WHERE target_task_id=$1) OR EXISTS(SELECT 1 FROM task_feature_links WHERE task_id=$1) OR EXISTS(SELECT 1 FROM task_feature_associations WHERE task_id=$1) OR EXISTS(SELECT 1 FROM task_approvals WHERE task_id=$1) OR EXISTS(SELECT 1 FROM pull_request_links WHERE task_id=$1) OR EXISTS(SELECT 1 FROM task_work_sessions WHERE task_id=$1) OR EXISTS(SELECT 1 FROM overtime_task_links WHERE task_id=$1)",&[&id]).map_err(Self::database_error)?.get(0);
        if anchored
            || before.backlog_id.is_some()
            || before.primary_requirement_id.is_some()
            || before.primary_feature_id.is_some()
        {
            return Err(PlatformError::Conflict(
                "task project is anchored by existing relationships",
            ));
        }
        if let Some(workflow) = &before.workflow_id {
            let bound: bool = tx
                .query_one(
                    "SELECT project_id IS NOT NULL FROM workflows WHERE id=$1",
                    &[&workflow],
                )
                .map_err(Self::database_error)?
                .get(0);
            if bound {
                return Err(PlatformError::Conflict("legacy workflow is project bound"));
            }
        }
        if let Some(project) = input.project_id.as_deref() {
            TaskPolicyStore::kind(&mut tx, project, &before.type_code)?;
        } else {
            tx.query_opt(
                "SELECT code FROM task_type_catalog WHERE code=$1 AND NOT archived FOR SHARE",
                &[&before.type_code],
            )
            .map_err(Self::database_error)?
            .ok_or(PlatformError::Validation("task type unavailable"))?;
        }
        if let Some(actor) = before.assignee_id.as_deref() {
            TaskScopeAuthority::assignee(&mut tx, input.project_id.as_deref(), actor)?;
        }
        let value:Value=tx.query_one("UPDATE tasks t SET project_id=$2,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(t)",&[&id,&input.project_id]).map_err(Self::database_error)?.get(0);
        // NULL scopes do not participate in a composite FK cascade.
        tx.execute("UPDATE task_history SET project_id=$2 WHERE task_id=$1 AND project_id IS DISTINCT FROM $2",&[&id,&input.project_id]).map_err(Self::database_error)?;
        tx.execute("UPDATE task_comments SET project_id=$2 WHERE task_id=$1 AND project_id IS DISTINCT FROM $2",&[&id,&input.project_id]).map_err(Self::database_error)?;
        tx.execute("INSERT INTO live_update_revisions(scope_id,topic) SELECT scope,'tasks' FROM unnest(ARRAY['*'::text,$1::text,$2::text]) scope WHERE scope IS NOT NULL GROUP BY scope ON CONFLICT(scope_id,topic) DO UPDATE SET revision=live_update_revisions.revision+1",&[&before.project_id,&input.project_id]).map_err(Self::database_error)?;
        let task: WorkTask = Self::decode(value.clone())?;
        TaskHistoryWriter::append(
            &mut tx,
            ctx,
            &task,
            "task.project_changed",
            Some(&before),
            Some(&reason),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(task)
    }
}
