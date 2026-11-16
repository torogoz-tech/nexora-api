use crate::PlatformStore;
use nexora_platform::*;

pub(crate) struct TaskCommentQueryStore;
impl TaskCommentQueryStore {
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        task: &str,
        after: Option<&str>,
        limit: u16,
    ) -> Result<TaskCommentPage, PlatformError> {
        if !(1..=100).contains(&limit) || after.is_some_and(|id| id.is_empty() || id.len() > 128) {
            return Err(PlatformError::Validation("invalid comment page"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        crate::task_scope_authority::TaskScopeAuthority::require(
            &mut tx,
            ctx,
            Capability::TasksRead,
            project,
        )
        .map_err(|error| {
            if matches!(error, PlatformError::Forbidden) {
                PlatformError::NotFound
            } else {
                error
            }
        })?;
        crate::task_scope_authority::TaskScopeAuthority::relation(&mut tx, ctx, project, task)?;
        tx.query_opt(
            "SELECT id FROM tasks WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2",
            &[&task, &project],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::NotFound)?;
        let sequence = if let Some(id) = after {
            tx.query_opt(
                "SELECT sequence FROM task_comments WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2 AND task_id=$3",
                &[&id, &project, &task],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::Validation("comment cursor is outside task"))?
            .get::<_, i64>(0)
        } else {
            0
        };
        let rows=tx.query("SELECT to_jsonb(c)||jsonb_build_object('actor_name',(SELECT name FROM actors WHERE id=c.actor_id)) FROM task_comments c WHERE project_id IS NOT DISTINCT FROM $1 AND task_id=$2 AND sequence>$3 ORDER BY sequence LIMIT $4",&[&project,&task,&sequence,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let mut items = rows
            .into_iter()
            .map(|row| PlatformStore::decode::<TaskComment>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next_after = if more {
            items.last().map(|item| item.id.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(TaskCommentPage { items, next_after })
    }
}
