use crate::{
    PlatformStore, SecretGenerator, receipt_writer::ReceiptWriter,
    task_dependency_policy::TaskDependencyPolicy, task_relation_lock::TaskRelationLock,
    task_relation_policy::TaskRelationPolicy,
};
use nexora_platform::*;
use serde_json::Value;
pub(crate) struct TaskRelationDependencyStore;
impl TaskRelationDependencyStore {
    pub fn create(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        input: &TaskDependencyCreate,
        project: Option<&str>,
    ) -> Result<TaskDependency, PlatformError> {
        if input.source_revision < 1 || input.target_revision < 1 {
            return Err(PlatformError::Validation("task revisions required"));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        TaskRelationLock::acquire(&mut tx)?;
        let tasks = TaskRelationPolicy::pair_allow_archived(
            &mut tx,
            ctx,
            &input.source_task_id,
            &input.target_task_id,
            true,
        )?;
        if project.is_some() && project != tasks.0.project_id.as_deref() {
            return Err(PlatformError::NotFound);
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "task.dependency_create",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        if tasks.0.archived || tasks.1.archived {
            return Err(PlatformError::Conflict("relation task is archived"));
        }
        if tasks.0.revision != input.source_revision || tasks.1.revision != input.target_revision {
            return Err(PlatformError::Conflict("dependency task revision changed"));
        }
        let (waiting, prerequisite) = if input.kind == TaskDependencyKind::DependsOn {
            (&tasks.0, &tasks.1)
        } else {
            (&tasks.1, &tasks.0)
        };
        if input.kind != TaskDependencyKind::RelatedTo {
            TaskRelationPolicy::cycle(&mut tx, &waiting.id, &prerequisite.id)?;
            if waiting.lifecycle != TaskLifecycle::Pending
                && prerequisite.lifecycle != TaskLifecycle::Completed
            {
                return Err(PlatformError::Conflict(
                    "cannot add unfinished prerequisite to started task",
                ));
            }
        }
        let duplicate=tx.query_one("SELECT EXISTS(SELECT 1 FROM task_dependencies WHERE status='active' AND removed_at IS NULL AND ((source_task_id=$1 AND target_task_id=$2 AND kind=$3) OR ($3='depends_on' AND kind='blocks' AND source_task_id=$2 AND target_task_id=$1) OR ($3='blocks' AND kind='depends_on' AND source_task_id=$2 AND target_task_id=$1) OR($3='related_to' AND kind='related_to' AND source_task_id=$2 AND target_task_id=$1)))",&[&tasks.0.id,&tasks.1.id,&input.kind.as_str()]).map_err(PlatformStore::database_error)?.get::<_,bool>(0);
        if duplicate {
            return Err(PlatformError::Conflict("dependency already exists"));
        }
        let id = SecretGenerator::id("tdp")?;
        let value:Value=tx.query_one("INSERT INTO task_dependencies AS d(id,project_id,source_task_id,target_task_id,kind,created_by) VALUES($1,$2,$3,$4,$5,$6) RETURNING to_jsonb(d)",&[&id,&tasks.0.project_id,&tasks.0.id,&tasks.1.id,&input.kind.as_str(),&ctx.actor_id()]).map_err(PlatformStore::database_error)?.get(0);
        let dependency: TaskDependency = PlatformStore::decode(value.clone())?;
        TaskDependencyPolicy::trace(
            &mut tx,
            ctx,
            &tasks,
            &dependency,
            "task.dependency_added",
            &reason,
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(dependency)
    }
    pub fn change(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        task: Option<&str>,
        project: Option<&str>,
        id: &str,
        input: &TaskDependencyChange,
        remove: bool,
    ) -> Result<TaskDependency, PlatformError> {
        if input.expected_revision < 1 {
            return Err(PlatformError::Validation("dependency revision required"));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let operation = if remove {
            "task.dependency_remove"
        } else {
            "task.dependency_resolve"
        };
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        TaskRelationLock::acquire(&mut tx)?;
        let value: Value = tx
            .query_opt(
                "SELECT to_jsonb(d) FROM task_dependencies d WHERE id=$1 FOR UPDATE",
                &[&id],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        let before: TaskDependency = PlatformStore::decode(value)?;
        if project.is_some() && project != before.project_id.as_deref()
            || task
                .is_some_and(|task| task != before.source_task_id && task != before.target_task_id)
        {
            return Err(PlatformError::NotFound);
        }
        let tasks = TaskRelationPolicy::pair_allow_archived(
            &mut tx,
            ctx,
            &before.source_task_id,
            &before.target_task_id,
            true,
        )?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            operation,
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        if !remove && (tasks.0.archived || tasks.1.archived) {
            return Err(PlatformError::Conflict("relation task is archived"));
        }
        if before.revision != input.expected_revision || before.removed_at.is_some() {
            return Err(PlatformError::Conflict(
                "dependency revision changed or removed",
            ));
        }
        if !remove {
            if before.status == TaskDependencyStatus::Resolved {
                return Err(PlatformError::Conflict("dependency already resolved"));
            }
            let blocker = if before.kind == TaskDependencyKind::DependsOn {
                &tasks.1
            } else {
                &tasks.0
            };
            if before.kind != TaskDependencyKind::RelatedTo
                && blocker.lifecycle != TaskLifecycle::Completed
            {
                return Err(PlatformError::Conflict(
                    "complete blocking task before resolving dependency",
                ));
            }
        }
        let sql = if remove {
            "UPDATE task_dependencies AS d SET removed_at=clock_timestamp(),removed_by=$2,removal_reason=$3,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(d)"
        } else {
            "UPDATE task_dependencies AS d SET status='resolved',resolved_at=clock_timestamp(),resolved_by=$2,resolution_reason=$3,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(d)"
        };
        let value: Value = tx
            .query_one(sql, &[&id, &ctx.actor_id(), &reason])
            .map_err(PlatformStore::database_error)?
            .get(0);
        let result: TaskDependency = PlatformStore::decode(value.clone())?;
        TaskDependencyPolicy::trace(
            &mut tx,
            ctx,
            &tasks,
            &result,
            if remove {
                "task.dependency_removed"
            } else {
                "task.dependency_resolved"
            },
            &reason,
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
