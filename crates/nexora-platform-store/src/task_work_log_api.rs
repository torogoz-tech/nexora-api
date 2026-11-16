use crate::{PlatformStore, task_work_log_store::TaskWorkLogStore};
use nexora_platform::*;
impl PlatformStore {
    pub fn migrate_pool_task_workflow(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &TaskWorkflowMigration,
    ) -> Result<WorkTask, PlatformError> {
        crate::task_pool_migration_store::TaskPoolMigrationStore::migrate(self, ctx, id, input)
    }

    pub fn task_work_logs(
        &mut self,
        ctx: &AuthContext,
        task: &str,
    ) -> Result<Vec<TaskWorkLog>, PlatformError> {
        TaskWorkLogStore::list(self, ctx, task)
    }
    pub fn save_task_work_log(
        &mut self,
        ctx: &AuthContext,
        task: &str,
        id: Option<&str>,
        input: &TaskWorkLogInput,
        remove: bool,
    ) -> Result<TaskWorkLog, PlatformError> {
        TaskWorkLogStore::mutate(self, ctx, task, id, input, remove)
    }
}
