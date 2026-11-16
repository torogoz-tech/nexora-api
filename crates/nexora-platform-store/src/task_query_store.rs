use crate::PlatformStore;
use crate::{task_read_authority::TaskReadAuthority, task_read_predicate::task_read_predicate};
use nexora_platform::*;
use serde_json::Value;

pub(crate) struct TaskQueryStore;
impl TaskQueryStore {
    fn authorize(
        tx: &mut postgres::Transaction<'_>,
        ctx: &AuthContext,
        project: Option<&str>,
    ) -> Result<(), PlatformError> {
        crate::task_scope_authority::TaskScopeAuthority::require(
            tx,
            ctx,
            Capability::TasksRead,
            project,
        )
        .map_err(|e| {
            if matches!(e, PlatformError::Forbidden) {
                PlatformError::NotFound
            } else {
                e
            }
        })?;
        Ok(())
    }
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        id: &str,
    ) -> Result<WorkTask, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authorize(&mut tx, ctx, project)?;
        crate::task_scope_authority::TaskScopeAuthority::relation(&mut tx, ctx, project, id)?;
        let value: Value = tx
            .query_opt(
                "SELECT to_jsonb(t) FROM tasks t WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2",
                &[&id, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?
            .get(0);
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn page(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        input: &TaskListQuery,
    ) -> Result<TaskPage, PlatformError> {
        if !(1..=100).contains(&input.limit)
            || input
                .after
                .as_ref()
                .is_some_and(|id| id.is_empty() || id.len() > 128)
        {
            return Err(PlatformError::Validation("invalid task page"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authorize(&mut tx, ctx, Some(project))?;
        if let Some(backlog) = &input.backlog_id {
            tx.query_opt(
                "SELECT id FROM backlogs WHERE id=$1 AND project_id=$2",
                &[&backlog, &project],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        }
        let after_position = if let (Some(backlog), Some(after)) = (&input.backlog_id, &input.after)
        {
            Some(
                tx.query_opt(
                    "SELECT position FROM tasks WHERE id=$1 AND project_id=$2 AND backlog_id=$3",
                    &[&after, &project, &backlog],
                )
                .map_err(PlatformStore::database_error)?
                .ok_or(PlatformError::Conflict("task page anchor changed"))?
                .get::<_, i64>(0),
            )
        } else {
            None
        };
        let priority = input.priority.map(TaskPriority::as_str);
        let unrestricted = TaskReadAuthority::unrestricted(&mut tx, ctx, project)?;
        let visibility = task_read_predicate("$12", "$13");
        let rows=tx.query(&format!("SELECT (to_jsonb(t)-'context'-'fields'-'resolution'-'origin_reference')||task_relation_counts(t.id) FROM tasks t WHERE project_id=$1 AND ($2::text IS NULL OR backlog_id=$2) AND ($3::text IS NULL OR assignee_id=$3) AND ($4::text IS NULL OR type_code=$4) AND ($5::text IS NULL OR priority=$5) AND ($6::text IS NULL OR state_id=$6) AND ($7::text IS NULL OR workflow_version_id=$7) AND ($8 OR NOT archived) AND ($9::text IS NULL OR ($2 IS NULL AND id>$9) OR ($2 IS NOT NULL AND (position,id)>($10,$9))) AND {visibility} ORDER BY CASE WHEN $2::text IS NOT NULL THEN position END,id LIMIT $11"),&[&project,&input.backlog_id,&input.assignee_id,&input.type_code,&priority,&input.state_id,&input.workflow_version_id,&input.include_archived,&input.after,&after_position,&(i64::from(input.limit)+1),&ctx.actor_id(),&unrestricted]).map_err(PlatformStore::database_error)?;
        let mut items = rows
            .into_iter()
            .map(|row| PlatformStore::decode::<TaskSummary>(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        let more = items.len() > usize::from(input.limit);
        items.truncate(usize::from(input.limit));
        let next_after = if more {
            items.last().map(|task| task.id.clone())
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(TaskPage { items, next_after })
    }
    pub fn history(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: Option<&str>,
        id: &str,
        after: Option<i64>,
        limit: u16,
    ) -> Result<TaskHistoryPage, PlatformError> {
        if !(1..=100).contains(&limit) || after.is_some_and(|value| value < 0) {
            return Err(PlatformError::Validation("invalid task history page"));
        }
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        Self::authorize(&mut tx, ctx, project)?;
        crate::task_scope_authority::TaskScopeAuthority::relation(&mut tx, ctx, project, id)?;
        tx.query_opt(
            "SELECT id FROM tasks WHERE id=$1 AND project_id IS NOT DISTINCT FROM $2",
            &[&id, &project],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::NotFound)?;
        let rows=tx.query("SELECT to_jsonb(h) FROM task_history h WHERE task_id=$1 AND project_id IS NOT DISTINCT FROM $2 AND ($3::bigint IS NULL OR sequence>$3) ORDER BY sequence LIMIT $4",&[&id,&project,&after,&(i64::from(limit)+1)]).map_err(PlatformStore::database_error)?;
        let mut items = Vec::new();
        let mut bytes = 0usize;
        let mut more = false;
        for row in rows {
            let value: Value = row.get(0);
            let cost = serde_json::to_vec(&value)
                .map_err(|_| PlatformError::Storage)?
                .len();
            if items.len() >= usize::from(limit) || (!items.is_empty() && bytes + cost > 262144) {
                more = true;
                break;
            }
            bytes += cost;
            items.push(PlatformStore::decode::<TaskHistoryEntry>(value)?);
        }
        let next_after = if more {
            items.last().map(|item| item.sequence)
        } else {
            None
        };
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(TaskHistoryPage { items, next_after })
    }
}
