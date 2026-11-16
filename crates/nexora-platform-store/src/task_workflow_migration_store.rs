use crate::{
    PlatformStore, platform_authority::PlatformAuthority, receipt_writer::ReceiptWriter,
    task_history_writer::TaskHistoryWriter, task_policy_store::TaskPolicyStore,
};
use nexora_platform::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub(crate) struct TaskWorkflowMigrationStore;
impl TaskWorkflowMigrationStore {
    pub fn migrate(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &TaskWorkflowMigration,
    ) -> Result<WorkTask, PlatformError> {
        let governed: bool = store
            .client
            .query_opt(
                "SELECT governed_stage FROM tasks WHERE id=$1 AND project_id=$2",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .is_some_and(|row| row.get(0));
        if governed {
            return crate::task_pool_migration_store::TaskPoolMigrationStore::migrate(
                store, ctx, id, input,
            );
        }
        if input.expected_revision < 1
            || input.state_mapping.is_empty()
            || input.state_mapping.len() > 64
        {
            return Err(PlatformError::Validation(
                "workflow migration requires revision and complete state map",
            ));
        }
        let reason = DomainText::required(&input.reason, 4000)?;
        let receipt = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        crate::task_relation_lock::TaskRelationLock::acquire(&mut tx)?;
        for capability in [
            Capability::TasksWrite,
            Capability::TasksTransition,
            Capability::WorkflowsPublish,
        ] {
            PlatformAuthority::require(&mut tx, ctx, capability, Some(project))?;
        }
        PlatformAuthority::project(&mut tx, project)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            receipt.as_ref(),
            ctx.actor_id(),
            "task.workflow_migrate",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let before = TaskPolicyStore::task(&mut tx, project, id)?;
        if before.archived || before.revision != input.expected_revision {
            return Err(PlatformError::Conflict("task revision changed or archived"));
        }
        if before.workflow_version_id.as_deref() == Some(input.workflow_version_id.as_str()) {
            return Err(PlatformError::Validation(
                "task already uses this workflow version",
            ));
        }
        let source = TaskPolicyStore::flow(&mut tx, project, before.execution_version()?, false)?;
        let target = TaskPolicyStore::flow(&mut tx, project, &input.workflow_version_id, true)?;
        if target.project_id.is_none() {
            let actor = before
                .assignee_id
                .as_deref()
                .ok_or(PlatformError::Validation("task needs a developer workflow"))?;
            let version = crate::developer_task_policy::DeveloperTaskPolicy::assigned_version(
                &mut tx,
                actor,
                &target.workflow_id,
            )?
            .ok_or(PlatformError::Validation("developer has no workflow"))?;
            if TaskPolicyStore::flow(&mut tx, project, &version, true)?.workflow_id
                != target.workflow_id
            {
                return Err(PlatformError::Validation(
                    "reassign the developer and destination state together",
                ));
            }
        }
        // Keep the destination assignable through commit without changing historical flow reads.
        tx.query_opt(
            "SELECT id FROM workflows WHERE id=$1 AND (project_id IS NULL OR project_id=$2) AND NOT archived FOR SHARE",
            &[&target.workflow_id, &project],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::Conflict("target workflow is archived"))?;
        let source_states: BTreeSet<&str> = source
            .graph
            .states
            .iter()
            .map(|state| state.id.as_str())
            .collect();
        let mapped_states: BTreeSet<&str> =
            input.state_mapping.keys().map(String::as_str).collect();
        let target_states: BTreeSet<&str> = target
            .graph
            .states
            .iter()
            .map(|state| state.id.as_str())
            .collect();
        if source_states != mapped_states
            || input
                .state_mapping
                .values()
                .any(|state| !target_states.contains(state.as_str()))
        {
            return Err(PlatformError::Validation(
                "migration must map every source state to an existing destination state",
            ));
        }
        let state =
            input
                .state_mapping
                .get(before.execution_state()?)
                .ok_or(PlatformError::Conflict(
                    "task state is outside its source workflow",
                ))?;
        crate::task_relation_policy::TaskRelationPolicy::state(
            &mut tx,
            &before,
            target
                .graph
                .states
                .iter()
                .any(|s| s.id == *state && s.terminal),
        )?;
        let value:Value=tx.query_one("UPDATE tasks AS t SET workflow_id=$3,workflow_version_id=$4,state_id=$5,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id=$2 RETURNING to_jsonb(t)",&[&id,&project,&target.workflow_id,&target.id,&state]).map_err(PlatformStore::database_error)?.get(0);
        let result: WorkTask = PlatformStore::decode(value.clone())?;
        TaskHistoryWriter::event(
            &mut tx,
            ctx,
            &result,
            "task.workflow_migrated",
            json!({"before":TaskHistoryWriter::state(&before)?,"after":TaskHistoryWriter::state(&result)?,"state_mapping":input.state_mapping,"reason":reason}),
        )?;
        let public = serde_json::to_value(&result).map_err(|_| PlatformError::Storage)?;
        ReceiptWriter::save(&mut tx, receipt.as_ref(), &public)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
