use crate::{
    PlatformStore, task_policy_store::TaskPolicyStore, task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::*;
use postgres::Transaction;
pub(crate) struct TaskRelationPolicy;
impl TaskRelationPolicy {
    pub fn pair_allow_archived(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        parent: &str,
        child: &str,
        allow_archived: bool,
    ) -> Result<(WorkTask, WorkTask), PlatformError> {
        if parent == child {
            return Err(PlatformError::Validation("task cannot relate to itself"));
        }
        let a = TaskScopeAuthority::task(tx, ctx, parent, Capability::TasksWrite)?;
        let b = TaskScopeAuthority::task(tx, ctx, child, Capability::TasksWrite)?;
        if a.project_id != b.project_id {
            return Err(PlatformError::Validation(
                "related tasks must share project scope",
            ));
        }
        for task in [&a, &b] {
            TaskScopeAuthority::modify(tx, ctx, task.project_id.as_deref(), &task.id, false)?;
        }
        let a = TaskPolicyStore::scoped_task(tx, a.project_id.as_deref(), parent)?;
        let b = TaskPolicyStore::scoped_task(tx, b.project_id.as_deref(), child)?;
        if !allow_archived && (a.archived || b.archived) {
            return Err(PlatformError::Conflict("relation task is archived"));
        }
        Ok((a, b))
    }
    pub fn blocking_count(tx: &mut Transaction<'_>, task: &str) -> Result<i64, PlatformError> {
        Ok(tx.query_one("SELECT count(*) FROM task_dependencies d JOIN tasks b ON b.id=CASE WHEN d.kind='depends_on' THEN d.target_task_id ELSE d.source_task_id END WHERE d.status='active' AND d.removed_at IS NULL AND ((d.kind='depends_on' AND d.source_task_id=$1) OR(d.kind='blocks' AND d.target_task_id=$1)) AND b.lifecycle<>'completed'",&[&task]).map_err(PlatformStore::database_error)?.get(0))
    }
    pub fn unfinished_children(tx: &mut Transaction<'_>, task: &str) -> Result<i64, PlatformError> {
        Ok(tx.query_one("SELECT count(*) FROM task_subtasks s JOIN tasks c ON c.id=s.child_task_id JOIN tasks p ON p.id=s.parent_task_id WHERE s.parent_task_id=$1 AND s.removed_at IS NULL AND c.lifecycle<>'completed' AND (NOT p.governed_stage OR ((s.stage_workflow_version_id IS NULL OR s.stage_workflow_version_id=p.workflow_version_id) AND NOT EXISTS(SELECT 1 FROM task_stage_completions done WHERE done.task_id=c.id AND done.workflow_version_id=COALESCE(s.stage_workflow_version_id,p.workflow_version_id) AND c.workflow_version_id=done.workflow_version_id AND EXISTS(SELECT 1 FROM workflow_versions cv,jsonb_array_elements(cv.graph->'states') cs WHERE cv.id=c.workflow_version_id AND cs->>'id'=c.state_id AND COALESCE((cs->>'terminal')::boolean,false)))))",&[&task]).map_err(PlatformStore::database_error)?.get(0))
    }
    pub fn take(tx: &mut Transaction<'_>, task: &WorkTask) -> Result<(), PlatformError> {
        if task.blocked || Self::blocking_count(tx, &task.id)? > 0 {
            return Err(PlatformError::Conflict(
                "complete blocking tasks before taking this task",
            ));
        }
        Ok(())
    }
    pub fn state(
        tx: &mut Transaction<'_>,
        task: &WorkTask,
        terminal: bool,
    ) -> Result<(), PlatformError> {
        if terminal && Self::unfinished_children(tx, &task.id)? > 0 {
            return Err(PlatformError::Conflict(
                "complete all subtasks before completing this task",
            ));
        }
        if !terminal && tx.query_one("SELECT EXISTS(SELECT 1 FROM task_subtasks s JOIN tasks p ON p.id=s.parent_task_id WHERE s.child_task_id=$1 AND s.removed_at IS NULL AND p.lifecycle='completed')",&[&task.id]).map_err(PlatformStore::database_error)?.get::<_,bool>(0){return Err(PlatformError::Conflict("cannot reopen subtask of completed parent"));}
        Ok(())
    }
    pub fn cycle(
        tx: &mut Transaction<'_>,
        waiting: &str,
        prerequisite: &str,
    ) -> Result<(), PlatformError> {
        if tx.query_one("WITH RECURSIVE reach(id) AS (SELECT $2::text UNION SELECT e.prerequisite FROM task_wait_edges e JOIN reach r ON e.waiting=r.id) SELECT EXISTS(SELECT 1 FROM reach WHERE id=$1)",&[&waiting,&prerequisite]).map_err(PlatformStore::database_error)?.get::<_,bool>(0){return Err(PlatformError::Validation("task relations would create a cycle"));}
        Ok(())
    }
}
