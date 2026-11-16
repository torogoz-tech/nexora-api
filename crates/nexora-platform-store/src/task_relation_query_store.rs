use crate::{
    PlatformStore, task_relation_lock::TaskRelationLock, task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::{Value, json};
pub(crate) struct TaskRelationQueryStore;
impl TaskRelationQueryStore {
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Value, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        TaskRelationLock::acquire(&mut tx)?;
        let result = Self::read(&mut tx, ctx, id)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn summary(task: &WorkTask, relation: Option<(&str, i64)>) -> Value {
        json!({"id":task.id,"title":task.title,"lifecycle":task.lifecycle,"project_id":task.project_id,"assignee_id":task.assignee_id,"revision":task.revision,"relation_id":relation.map(|r|r.0),"relation_revision":relation.map(|r|r.1)})
    }
    pub fn read(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Value, PlatformError> {
        let task = TaskScopeAuthority::task(tx, ctx, id, Capability::TasksRead)?;
        let can_manage = match TaskScopeAuthority::require(
            tx,
            ctx,
            Capability::TasksWrite,
            task.project_id.as_deref(),
        )
        .and_then(|_| TaskScopeAuthority::modify(tx, ctx, task.project_id.as_deref(), id, false))
        {
            Ok(()) => !task.archived,
            Err(PlatformError::Forbidden | PlatformError::NotFound) => false,
            Err(e) => return Err(e),
        };
        let rows=tx.query("SELECT parent_task_id,child_task_id,work_kind,stage_workflow_version_id,tested_commit_sha,tested_repository_id FROM task_subtasks WHERE removed_at IS NULL AND (parent_task_id=$1 OR child_task_id=$1) ORDER BY created_at,id",&[&id]).map_err(PlatformStore::database_error)?;
        let mut parent = None;
        let mut subtasks = Vec::new();
        let mut total = 0i64;
        let mut completed = 0i64;
        for row in rows {
            let p: String = row.get(0);
            let c: String = row.get(1);
            let related = if p == id { &c } else { &p };
            if p == id {
                total += 1;
                let state: String = tx
                    .query_one("SELECT lifecycle FROM tasks WHERE id=$1", &[related])
                    .map_err(PlatformStore::database_error)?
                    .get(0);
                let stage: Option<String> = row.get(3);
                let stage_done:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM task_stage_completions done JOIN tasks child ON child.id=done.task_id JOIN workflow_versions cv ON cv.id=child.workflow_version_id CROSS JOIN LATERAL jsonb_array_elements(cv.graph->'states') cs WHERE done.task_id=$1 AND done.workflow_version_id=$2 AND child.workflow_version_id=done.workflow_version_id AND cs->>'id'=child.state_id AND COALESCE((cs->>'terminal')::boolean,false))",&[related,&stage]).map_err(PlatformStore::database_error)?.get(0);
                if state == "completed" || stage_done {
                    completed += 1;
                }
            }
            match TaskScopeAuthority::task(tx, ctx, related, Capability::TasksRead) {
                Ok(value) => {
                    if p == id {
                        let mut summary = Self::summary(&value, None);
                        let stage: Option<String> = row.get(3);
                        let stage_done:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM task_stage_completions done JOIN tasks child ON child.id=done.task_id JOIN workflow_versions cv ON cv.id=child.workflow_version_id CROSS JOIN LATERAL jsonb_array_elements(cv.graph->'states') cs WHERE done.task_id=$1 AND done.workflow_version_id=$2 AND child.workflow_version_id=done.workflow_version_id AND cs->>'id'=child.state_id AND COALESCE((cs->>'terminal')::boolean,false))",&[&value.id,&stage]).map_err(PlatformStore::database_error)?.get(0);
                        summary["stage_completed"] =
                            json!(value.lifecycle == TaskLifecycle::Completed || stage_done);
                        summary["work_kind"] = json!(row.get::<_, String>(2));
                        summary["stage_workflow_version_id"] =
                            json!(row.get::<_, Option<String>>(3));
                        summary["tested_commit_sha"] = json!(row.get::<_, Option<String>>(4));
                        summary["tested_repository_id"] = json!(row.get::<_, Option<String>>(5));
                        subtasks.push(summary)
                    } else {
                        parent = Some(Self::summary(&value, None))
                    }
                }
                Err(PlatformError::Forbidden | PlatformError::NotFound) => {}
                Err(e) => return Err(e),
            }
        }
        let rows=tx.query("SELECT id,revision,CASE WHEN kind='depends_on' THEN source_task_id ELSE target_task_id END,CASE WHEN kind='depends_on' THEN target_task_id ELSE source_task_id END FROM task_dependencies WHERE status='active' AND removed_at IS NULL AND kind IN ('blocks','depends_on') AND (source_task_id=$1 OR target_task_id=$1) ORDER BY created_at,id",&[&id]).map_err(PlatformStore::database_error)?;
        let mut blocked_by = Vec::new();
        let mut blocks = Vec::new();
        let mut blocking = 0i64;
        for row in rows {
            let relation: String = row.get(0);
            let revision: i64 = row.get(1);
            let waiting: String = row.get(2);
            let prerequisite: String = row.get(3);
            let related = if waiting == id {
                &prerequisite
            } else {
                &waiting
            };
            if waiting == id {
                let state: String = tx
                    .query_one("SELECT lifecycle FROM tasks WHERE id=$1", &[related])
                    .map_err(PlatformStore::database_error)?
                    .get(0);
                if state != "completed" {
                    blocking += 1;
                }
            }
            match TaskScopeAuthority::task(tx, ctx, related, Capability::TasksRead) {
                Ok(value) => {
                    if waiting == id {
                        blocked_by.push(Self::summary(&value, Some((&relation, revision))))
                    } else {
                        blocks.push(Self::summary(&value, Some((&relation, revision))))
                    }
                }
                Err(PlatformError::Forbidden | PlatformError::NotFound) => {}
                Err(e) => return Err(e),
            }
        }
        let initial_terminal:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM developer_workflows d JOIN workflow_versions v ON v.id=d.workflow_version_id CROSS JOIN LATERAL jsonb_array_elements(v.graph->'states') s WHERE d.actor_id=$1 AND COALESCE((s->>'initial')::boolean,false) AND COALESCE((s->>'terminal')::boolean,false)) AND NOT EXISTS(SELECT 1 FROM developer_workflows d JOIN workflows w ON w.id=d.workflow_id JOIN workflow_versions v ON v.id=d.workflow_version_id CROSS JOIN LATERAL jsonb_array_elements(v.graph->'states') s WHERE d.actor_id=$1 AND NOT w.archived AND COALESCE((s->>'initial')::boolean,false) AND NOT COALESCE((s->>'terminal')::boolean,false))",&[&ctx.actor_id()]).map_err(PlatformStore::database_error)?.get(0);
        Ok(
            json!({"task_revision":task.revision,"parent":parent,"subtasks":subtasks,"blocked_by":blocked_by,"blocks":blocks,"can_manage":can_manage,"can_take":blocking==0&&!task.blocked&&!(initial_terminal&&total>completed),"can_complete":total==completed,"subtasks_total":total,"subtasks_completed":completed,"unfinished_subtasks":total-completed,"blocking_count":blocking}),
        )
    }
}
