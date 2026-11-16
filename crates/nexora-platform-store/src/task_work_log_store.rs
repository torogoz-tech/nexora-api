use crate::{
    PlatformStore, SecretGenerator, receipt_writer::ReceiptWriter,
    task_history_writer::TaskHistoryWriter, task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::*;
use serde_json::{Value, json};
pub(crate) struct TaskWorkLogStore;
impl TaskWorkLogStore {
    pub fn list(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        task: &str,
    ) -> Result<Vec<TaskWorkLog>, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        TaskScopeAuthority::task(&mut tx, ctx, task, Capability::TasksRead)?;
        let rows=tx.query("SELECT to_jsonb(w)||jsonb_build_object('actor_name',(SELECT name FROM actors WHERE id=w.actor_id)) FROM task_work_logs w WHERE task_id=$1 AND removed_at IS NULL ORDER BY date DESC,id",&[&task]).map_err(PlatformStore::database_error)?;
        let result = rows
            .into_iter()
            .map(|row| PlatformStore::decode(row.get(0)))
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
    pub fn mutate(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        task: &str,
        id: Option<&str>,
        input: &TaskWorkLogInput,
        remove: bool,
    ) -> Result<TaskWorkLog, PlatformError> {
        if remove && id.is_none() {
            return Err(PlatformError::Validation(
                "work log ID is required for removal",
            ));
        }
        if !(1..=1440).contains(&input.minutes) {
            return Err(PlatformError::Validation(
                "work minutes must be between 1 and 1440",
            ));
        }
        crate::labor_calendar_policy::LaborCalendarPolicy::date(
            &json!({"date":input.date}),
            "date",
        )?;
        let note = input
            .note
            .as_deref()
            .map(|value| DomainText::required(value, 4000))
            .transpose()?;
        let operation = if remove {
            "task.work_log_removed"
        } else if id.is_some() {
            "task.work_log_changed"
        } else {
            "task.work_log_added"
        };
        let request = store.request_context.clone();
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let resource = TaskScopeAuthority::task(&mut tx, ctx, task, Capability::TasksWrite)?;
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &store.namespace,
            request.as_ref(),
            ctx.actor_id(),
            operation,
        )? {
            tx.commit().map_err(PlatformStore::database_error)?;
            return PlatformStore::decode(value);
        }
        if resource.archived {
            return Err(PlatformError::Conflict("task is archived"));
        }
        let previous: Option<Value> = if let Some(id) = id {
            Some(tx.query_opt("SELECT to_jsonb(w) FROM task_work_logs w WHERE id=$1 AND task_id=$2 AND actor_id=$3 AND removed_at IS NULL FOR UPDATE",&[&id,&task,&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0))
        } else {
            None
        };
        if let Some(before) = &previous {
            if !before["title"].is_null() {
                return Err(PlatformError::Conflict(
                    "edit calendar work blocks from the calendar",
                ));
            }
            if before["revision"].as_i64() != input.expected_revision {
                return Err(PlatformError::Conflict("work log revision changed"));
            }
        }
        if !remove && let Some(overtime) = &input.overtime_request_id {
            let approved:i32=tx.query_opt("SELECT minutes FROM labor_overtime_requests WHERE id=$1 AND actor_id=$2 AND status='approved' AND $3::text::date BETWEEN date AND date+CASE WHEN next_day THEN 1 ELSE 0 END FOR UPDATE",&[&overtime,&ctx.actor_id(),&input.date]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Validation("overtime must be approved for this actor and work date"))?.get(0);
            let used:i64=tx.query_one("SELECT COALESCE(sum(minutes),0)::bigint FROM task_work_logs WHERE overtime_request_id=$1 AND removed_at IS NULL AND ($2::text IS NULL OR id<>$2)",&[&overtime,&id]).map_err(PlatformStore::database_error)?.get(0);
            if used + i64::from(input.minutes) > i64::from(approved) {
                return Err(PlatformError::Validation(
                    "work log exceeds approved overtime minutes",
                ));
            }
        }
        let value: Value = if let Some(id) = id {
            tx.query_one("UPDATE task_work_logs AS w SET date=CASE WHEN $7 THEN date ELSE $4::text::date END,minutes=CASE WHEN $7 THEN minutes ELSE $5 END,note=CASE WHEN $7 THEN note ELSE $6 END,overtime_request_id=CASE WHEN $7 THEN overtime_request_id ELSE $8 END,revision=revision+1,updated_at=clock_timestamp(),removed_at=CASE WHEN $7 THEN clock_timestamp() ELSE NULL END WHERE id=$1 AND task_id=$2 AND actor_id=$3 RETURNING to_jsonb(w)",&[&id,&task,&ctx.actor_id(),&input.date,&input.minutes,&note,&remove,&input.overtime_request_id]).map_err(PlatformStore::database_error)?.get(0)
        } else {
            let id = SecretGenerator::id("twl")?;
            tx.query_one("INSERT INTO task_work_logs AS w(id,task_id,actor_id,date,minutes,note,overtime_request_id) VALUES($1,$2,$3,$4::text::date,$5,$6,$7) RETURNING to_jsonb(w)",&[&id,&task,&ctx.actor_id(),&input.date,&input.minutes,&note,&input.overtime_request_id]).map_err(PlatformStore::database_error)?.get(0)
        };
        TaskHistoryWriter::event(
            &mut tx,
            ctx,
            &resource,
            operation,
            json!({"before_work_log":previous,"work_log":value}),
        )?;
        ReceiptWriter::save(&mut tx, request.as_ref(), &value)?;
        let result = PlatformStore::decode(value)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(result)
    }
}
