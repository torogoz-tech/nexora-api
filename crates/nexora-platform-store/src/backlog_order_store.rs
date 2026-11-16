use crate::{
    PlatformStore, audit_writer::AuditWriter, platform_authority::PlatformAuthority,
    receipt_writer::ReceiptWriter, task_history_writer::TaskHistoryWriter,
    task_policy_store::TaskPolicyStore,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::{Value, json};

pub(crate) struct BacklogOrderStore;
impl BacklogOrderStore {
    pub fn reorder(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        id: &str,
        input: &BacklogReorder,
    ) -> Result<Backlog, PlatformError> {
        if input.expected_revision < 1
            || (input.before_id.is_some() && input.after_id.is_some())
            || input.before_id.as_deref() == Some(&input.task_id)
            || input.after_id.as_deref() == Some(&input.task_id)
        {
            return Err(PlatformError::Validation(
                "invalid backlog ordering request",
            ));
        }
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        PlatformAuthority::require(&mut tx, ctx, Capability::BacklogsWrite, Some(project))?;
        PlatformAuthority::project(&mut tx, project)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "backlog.reorder",
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        let backlog = TaskPolicyStore::backlog(&mut tx, project, id)?;
        if backlog.revision != input.expected_revision {
            return Err(PlatformError::Conflict("backlog revision changed"));
        }
        if backlog.archived {
            return Err(PlatformError::Conflict("backlog is archived"));
        }
        let task = TaskPolicyStore::task(&mut tx, project, &input.task_id)?;
        if task.backlog_id.as_deref() != Some(id) || task.archived {
            return Err(PlatformError::Validation(
                "task is not active in this backlog",
            ));
        }
        let mut position = Self::position(&mut tx, project, id, input)?;
        if position.is_none() {
            // Internal integer ranks are rebalanced only when the selected gap is
            // exhausted. Client input never sets ranks or floating-point positions.
            tx.execute("WITH ranks AS (SELECT id,row_number() OVER(ORDER BY position,id)*1024 AS rank FROM tasks WHERE project_id=$1 AND backlog_id=$2) UPDATE tasks t SET position=r.rank FROM ranks r WHERE t.id=r.id",&[&project,&id]).map_err(PlatformStore::database_error)?;
            position = Self::position(&mut tx, project, id, input)?;
        }
        let position = position.ok_or(PlatformError::Conflict("backlog ordering exhausted"))?;
        let value:Value=tx.query_one("UPDATE tasks AS t SET position=$3 WHERE id=$1 AND project_id=$2 RETURNING to_jsonb(t)",&[&task.id,&project,&position]).map_err(PlatformStore::database_error)?.get(0);
        let ordered: WorkTask = PlatformStore::decode(value)?;
        TaskHistoryWriter::append(&mut tx, ctx, &ordered, "task.reordered", Some(&task), None)?;
        let value:Value=tx.query_one("UPDATE backlogs AS b SET revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 AND project_id=$2 RETURNING to_jsonb(b)",&[&id,&project]).map_err(PlatformStore::database_error)?.get(0);
        AuditWriter::append(
            &mut tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            "backlog.reordered",
            Some(id),
            json!({"project_id":project,"task_id":task.id,"revision":value["revision"]}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    fn position(
        tx: &mut Transaction<'_>,
        project: &str,
        backlog: &str,
        input: &BacklogReorder,
    ) -> Result<Option<i64>, PlatformError> {
        let (lower, upper) = if let Some(anchor) =
            input.before_id.as_ref().or(input.after_id.as_ref())
        {
            let row = tx
                .query_opt(
                    "SELECT position FROM tasks WHERE id=$1 AND project_id=$2 AND backlog_id=$3",
                    &[&anchor, &project, &backlog],
                )
                .map_err(PlatformStore::database_error)?
                .ok_or(PlatformError::Validation(
                    "ordering anchor is outside backlog",
                ))?;
            let anchor: i64 = row.get(0);
            if input.before_id.is_some() {
                let lower:Option<i64>=tx.query_one("SELECT max(position) FROM tasks WHERE project_id=$1 AND backlog_id=$2 AND id<>$3 AND position<$4",&[&project,&backlog,&input.task_id,&anchor]).map_err(PlatformStore::database_error)?.get(0);
                (lower, Some(anchor))
            } else {
                let upper:Option<i64>=tx.query_one("SELECT min(position) FROM tasks WHERE project_id=$1 AND backlog_id=$2 AND id<>$3 AND position>$4",&[&project,&backlog,&input.task_id,&anchor]).map_err(PlatformStore::database_error)?.get(0);
                (Some(anchor), upper)
            }
        } else {
            let lower:Option<i64>=tx.query_one("SELECT max(position) FROM tasks WHERE project_id=$1 AND backlog_id=$2 AND id<>$3",&[&project,&backlog,&input.task_id]).map_err(PlatformStore::database_error)?.get(0);
            (lower, None)
        };
        Ok(match (lower, upper) {
            (None, None) => Some(1024),
            (Some(lower), None) => lower.checked_add(1024),
            (None, Some(upper)) => upper.checked_sub(1024),
            (Some(lower), Some(upper)) => {
                let gap = i128::from(upper) - i128::from(lower);
                if gap > 1 {
                    i64::try_from(i128::from(lower) + gap / 2).ok()
                } else {
                    None
                }
            }
        })
    }
}
