use crate::{
    PlatformStore, SecretGenerator, receipt_writer::ReceiptWriter,
    task_history_writer::TaskHistoryWriter, task_policy_store::TaskPolicyStore,
    task_relation_lock::TaskRelationLock, task_relation_policy::TaskRelationPolicy,
    task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct TaskSubtaskStore;
impl TaskSubtaskStore {
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        parent: &str,
        input: &TaskSubtaskCreate,
    ) -> Result<WorkTask, PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation("parent revision required"));
        }
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        TaskRelationLock::acquire(&mut tx)?;
        let task = TaskScopeAuthority::task(&mut tx, ctx, parent, Capability::TasksWrite)?;
        TaskScopeAuthority::modify(&mut tx, ctx, task.project_id.as_deref(), parent, false)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.subtask_create",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let task = TaskPolicyStore::scoped_task(&mut tx, task.project_id.as_deref(), parent)?;
        if task.revision != input.expected_revision || task.archived {
            return Err(PlatformError::Conflict(
                "parent task revision changed or archived",
            ));
        }
        if task.lifecycle == TaskLifecycle::Completed {
            return Err(PlatformError::Conflict(
                "completed parent cannot have unfinished subtasks",
            ));
        }
        let corrective = input.work_kind == TaskSubtaskKind::Corrective;
        if !corrective
            && input
                .definition
                .entry_workflow_id
                .as_deref()
                .is_some_and(|entry| {
                    task.workflow_id
                        .as_deref()
                        .or(task.entry_workflow_id.as_deref())
                        .is_some_and(|parent_entry| entry != parent_entry)
                })
        {
            return Err(PlatformError::Validation(
                "decomposition subtasks must execute in the parent task's workflow stage",
            ));
        }
        if corrective {
            let repository =
                input
                    .tested_repository_id
                    .as_deref()
                    .ok_or(PlatformError::Validation(
                        "corrective subtask requires the tested repository",
                    ))?;
            let sha = input
                .tested_commit_sha
                .as_deref()
                .ok_or(PlatformError::Validation(
                    "corrective subtask requires the exact tested commit",
                ))?;
            if !TaskWorkBranchReport::valid_sha(sha)
                || task.workflow_id.is_none()
                || input.definition.entry_workflow_id.is_none()
            {
                return Err(PlatformError::Validation(
                    "corrective work requires a tested revision and destination workflow",
                ));
            }
            if input.definition.entry_workflow_id == task.workflow_id {
                return Err(PlatformError::Validation(
                    "corrective work must start in a different workflow before returning to the parent's tested stage",
                ));
            }
            let project = task.project_id.as_deref().ok_or(PlatformError::Validation(
                "corrective code work requires a project",
            ))?;
            crate::task_transition_provider_authority::TaskTransitionProviderAuthority::lock(
                &mut tx, ctx, project, repository,
            )?;
            crate::task_tested_revision::TaskTestedRevision::validate(
                &mut tx, &task, repository, sha,
            )?;
        } else if input.tested_commit_sha.is_some() || input.tested_repository_id.is_some() {
            return Err(PlatformError::Validation(
                "tested revision is only valid for corrective subtasks",
            ));
        }
        let child = crate::task_pool_store::TaskPoolStore::insert(
            &mut tx,
            ctx,
            task.project_id.as_deref(),
            &input.definition,
            None,
        )?;
        let link = SecretGenerator::id("tst")?;
        tx.execute("INSERT INTO task_subtasks(id,project_id,parent_task_id,child_task_id,created_by,stage_workflow_version_id,work_kind,tested_repository_id,tested_commit_sha) VALUES($1,$2,$3,$4,$5,(SELECT workflow_version_id FROM tasks WHERE id=$3),$6,$7,$8)",&[&link,&task.project_id,&parent,&child.id,&ctx.actor_id(),&if corrective{"corrective"}else{"decomposition"},&input.tested_repository_id,&input.tested_commit_sha]).map_err(PlatformStore::database_error)?;
        Self::trace(
            &mut tx,
            ctx,
            &task,
            &child,
            "task.subtask_added",
            "Subtask created",
        )?;
        let child = TaskPolicyStore::scoped_task(&mut tx, child.project_id.as_deref(), &child.id)?;
        let value = serde_json::to_value(&child).map_err(|_| PlatformError::Storage)?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(child)
    }
    pub fn change(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        parent: &str,
        child: &str,
        input: &TaskSubtaskChange,
        remove: bool,
    ) -> Result<Value, PlatformError> {
        if input.expected_revision < 1 || input.child_revision < 1 {
            return Err(PlatformError::Validation("task revisions required"));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let operation = if remove {
            "task.subtask_remove"
        } else {
            "task.subtask_link"
        };
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        TaskRelationLock::acquire(&mut tx)?;
        let (a, b) = TaskRelationPolicy::pair_allow_archived(&mut tx, ctx, parent, child, true)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            operation,
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return Ok(value);
        }
        if !remove && (a.archived || b.archived) {
            return Err(PlatformError::Conflict("relation task is archived"));
        }
        if a.revision != input.expected_revision || b.revision != input.child_revision {
            return Err(PlatformError::Conflict("related task revision changed"));
        }
        if remove {
            if tx.execute("UPDATE task_subtasks SET removed_at=clock_timestamp(),removed_by=$3,removal_reason=$4 WHERE parent_task_id=$1 AND child_task_id=$2 AND removed_at IS NULL",&[&parent,&child,&ctx.actor_id(),&reason]).map_err(PlatformStore::database_error)?!=1{return Err(PlatformError::NotFound);}
        } else {
            if b.purpose != TaskPurpose::General
                && (b.workflow_id.as_deref().or(b.entry_workflow_id.as_deref())
                    != a.workflow_id.as_deref().or(a.entry_workflow_id.as_deref())
                    || (b.workflow_version_id.is_some()
                        && a.workflow_version_id.is_some()
                        && b.workflow_version_id != a.workflow_version_id))
            {
                return Err(PlatformError::Validation(
                    "linked decomposition subtask must belong to the parent task's pinned workflow stage",
                ));
            }
            if tx.query_one("SELECT EXISTS(SELECT 1 FROM task_subtasks WHERE child_task_id=$1 AND removed_at IS NULL)",&[&child]).map_err(PlatformStore::database_error)?.get::<_,bool>(0){return Err(PlatformError::Conflict("subtask already has a parent"));}
            if a.lifecycle == TaskLifecycle::Completed && b.lifecycle != TaskLifecycle::Completed {
                return Err(PlatformError::Conflict(
                    "completed parent cannot have unfinished subtasks",
                ));
            }
            TaskRelationPolicy::cycle(&mut tx, parent, child)?;
            let link = SecretGenerator::id("tst")?;
            tx.execute("INSERT INTO task_subtasks(id,project_id,parent_task_id,child_task_id,created_by,stage_workflow_version_id) VALUES($1,$2,$3,$4,$5,(SELECT workflow_version_id FROM tasks WHERE id=$3))",&[&link,&a.project_id,&parent,&child,&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        }
        Self::trace(
            &mut tx,
            ctx,
            &a,
            &b,
            if remove {
                "task.subtask_removed"
            } else {
                "task.subtask_added"
            },
            &reason,
        )?;
        let parent_after = TaskPolicyStore::scoped_task(&mut tx, a.project_id.as_deref(), parent)?;
        let child_after = TaskPolicyStore::scoped_task(&mut tx, b.project_id.as_deref(), child)?;
        let value = json!({"parent_task_id":parent,"child_task_id":child,"linked":!remove,"parent_revision":parent_after.revision,"child_revision":child_after.revision});
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(value)
    }
    fn trace(
        tx: &mut postgres::Transaction<'_>,
        ctx: &AuthContext,
        parent: &WorkTask,
        child: &WorkTask,
        operation: &str,
        reason: &str,
    ) -> Result<(), PlatformError> {
        for before in [parent, child] {
            let value:Value=tx.query_one("UPDATE tasks AS t SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(t)",&[&before.id]).map_err(PlatformStore::database_error)?.get(0);
            let after: WorkTask = PlatformStore::decode(value)?;
            TaskHistoryWriter::event(
                tx,
                ctx,
                &after,
                operation,
                json!({"parent_task_id":parent.id,"child_task_id":child.id,"before":TaskHistoryWriter::state(before)?,"after":TaskHistoryWriter::state(&after)?,"reason":reason}),
            )?;
        }
        Ok(())
    }
}
