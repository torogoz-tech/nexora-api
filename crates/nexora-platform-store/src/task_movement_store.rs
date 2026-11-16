use crate::{
    PlatformStore, platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
    task_dependency_lock::TaskDependencyLock, task_history_writer::TaskHistoryWriter,
    task_policy_store::TaskPolicyStore,
};
use nexora_platform::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub(crate) struct TaskMovementStore;
impl TaskMovementStore {
    pub fn move_task(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &TaskMove,
    ) -> Result<WorkTask, PlatformError> {
        if input.expected_revision < 1
            || input.expected_target_backlog_revision < 1
            || input
                .expected_source_backlog_revision
                .is_some_and(|revision| revision < 1)
        {
            return Err(PlatformError::Validation(
                "movement requires positive expected revisions",
            ));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let destination = DomainText::required(&input.backlog_id, 128)?;
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        for capability in [Capability::TasksWrite, Capability::BacklogsWrite] {
            PlatformAuthority::require(&mut tx, ctx, capability, Some(project))?;
        }
        PlatformAuthority::project(&mut tx, project)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "task.move",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        TaskDependencyLock::shared(&mut tx, project)?;
        let value: Value = tx
            .query_opt(
                "SELECT to_jsonb(t) FROM tasks t WHERE id=$1 AND project_id=$2",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        let observed: WorkTask = PlatformStore::decode(value)?;
        if observed.revision != input.expected_revision {
            return Err(PlatformError::Conflict("task revision changed"));
        }
        if observed.backlog_id.as_deref() == Some(destination.as_str()) {
            return Err(PlatformError::Validation(
                "task is already in destination backlog",
            ));
        }
        if observed.backlog_id.is_some() != input.expected_source_backlog_revision.is_some() {
            return Err(PlatformError::Validation(
                "source backlog revision must match task assignment",
            ));
        }
        let mut ids = vec![destination.as_str()];
        if let Some(source) = observed.backlog_id.as_deref() {
            ids.push(source);
        }
        ids.sort_unstable();
        ids.dedup();
        let mut backlogs = BTreeMap::new();
        for backlog in ids {
            let row = TaskPolicyStore::backlog(&mut tx, project, backlog)?;
            backlogs.insert(row.id.clone(), row);
        }
        let before = TaskPolicyStore::task(&mut tx, project, id)?;
        if before.revision != input.expected_revision
            || before.backlog_id != observed.backlog_id
            || before.archived
        {
            return Err(PlatformError::Conflict(
                "task changed assignment, revision or archived state",
            ));
        }
        let target = backlogs.get(&destination).ok_or(PlatformError::Storage)?;
        let source = before.backlog_id.as_ref().and_then(|id| backlogs.get(id));
        if target.archived
            || target.revision != input.expected_target_backlog_revision
            || source.map(|row| row.revision) != input.expected_source_backlog_revision
        {
            return Err(PlatformError::Conflict(
                "backlog revision changed or destination archived",
            ));
        }
        let last: Option<i64> = tx
            .query_one(
                "SELECT max(position) FROM tasks WHERE project_id=$1 AND backlog_id=$2",
                &[&project, &destination],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        let position = last
            .unwrap_or(0)
            .checked_add(1024)
            .ok_or(PlatformError::Conflict("backlog ordering exhausted"))?;
        let value:Value=tx.query_one("UPDATE tasks AS t SET backlog_id=$3,position=$4,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id=$2 RETURNING to_jsonb(t)",&[&id,&project,&destination,&position]).map_err(PlatformStore::database_error)?.get(0);
        let result: WorkTask = PlatformStore::decode(value.clone())?;
        for backlog in backlogs.values() {
            tx.execute("UPDATE backlogs SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id=$2",&[&backlog.id,&project]).map_err(PlatformStore::database_error)?;
        }
        TaskHistoryWriter::event(
            &mut tx,
            ctx,
            &result,
            "task.moved",
            json!({"before":TaskHistoryWriter::state(&before)?,"after":TaskHistoryWriter::state(&result)?,"reason":reason,"backlog_revisions":backlogs.values().map(|row|json!({"backlog_id":row.id,"before":row.revision,"after":row.revision+1})).collect::<Vec<_>>()}),
        )?;
        let public = serde_json::to_value(&result).map_err(|_| PlatformError::Storage)?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &public)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
