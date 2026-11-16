use crate::{PlatformStore, task_history_writer::TaskHistoryWriter};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::{Value, json};

pub(crate) struct TaskDependencyPolicy;
impl TaskDependencyPolicy {
    pub fn trace(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        tasks: &(WorkTask, WorkTask),
        dependency: &TaskDependency,
        operation: &str,
        reason: &str,
    ) -> Result<(), PlatformError> {
        for before in [&tasks.0, &tasks.1] {
            let value:Value=tx.query_one("UPDATE tasks AS t SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2 RETURNING to_jsonb(t)",&[&before.id,&before.project_id]).map_err(PlatformStore::database_error)?.get(0);
            let after: WorkTask = PlatformStore::decode(value)?;
            TaskHistoryWriter::event(
                tx,
                ctx,
                &after,
                operation,
                json!({"dependency":dependency,"before":TaskHistoryWriter::state(before)?,"after":TaskHistoryWriter::state(&after)?,"reason":reason}),
            )?;
        }
        Ok(())
    }
}
