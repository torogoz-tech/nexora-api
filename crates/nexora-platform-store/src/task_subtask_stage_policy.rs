use crate::PlatformStore;
use nexora_platform::*;
pub(crate) struct TaskSubtaskStagePolicy;
impl TaskSubtaskStagePolicy {
    pub fn validate(
        tx: &mut postgres::Transaction<'_>,
        task: &str,
        workflow: Option<&str>,
        version: Option<&str>,
    ) -> Result<(), PlatformError> {
        let row=tx.query_opt("SELECT p.workflow_id,p.entry_workflow_id,p.workflow_version_id FROM task_subtasks s JOIN tasks p ON p.id=s.parent_task_id WHERE s.child_task_id=$1 AND s.removed_at IS NULL AND s.work_kind='decomposition' FOR SHARE OF p",&[&task]).map_err(PlatformStore::database_error)?;
        if let Some(parent) = row {
            let expected = parent.get::<_, Option<String>>(0).or(parent.get(1));
            let pinned: Option<String> = parent.get(2);
            if workflow.is_some() && workflow != expected.as_deref() {
                return Err(PlatformError::Conflict(
                    "decomposition subtask must use its parent workflow",
                ));
            }
            if version.is_some() && version != pinned.as_deref() {
                return Err(PlatformError::Conflict(
                    "take the parent first and use its pinned workflow version for decomposition work",
                ));
            }
        }
        Ok(())
    }
}
