use crate::{
    PlatformStore, SecretGenerator, labor_calendar_policy::LaborCalendarPolicy as Policy,
    receipt_writer::ReceiptWriter, task_history_writer::TaskHistoryWriter,
    task_scope_authority::TaskScopeAuthority,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn personal_work_blocks(
        &mut self,
        ctx: &AuthContext,
        from: &str,
        to: &str,
    ) -> Result<Value, PlatformError> {
        Policy::date(&json!({"from":from}), "from")?;
        Policy::date(&json!({"to":to}), "to")?;
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        if tx
            .query_one(
                "SELECT $2::text::date-$1::text::date NOT BETWEEN 0 AND 62",
                &[&from, &to],
            )
            .map_err(Self::database_error)?
            .get::<_, bool>(0)
        {
            return Err(PlatformError::Validation(
                "choose a range of at most 63 days",
            ));
        }
        let items:Vec<Value>=tx.query("SELECT to_jsonb(w) FROM task_work_logs w WHERE actor_id=$1 AND title IS NOT NULL AND removed_at IS NULL AND date BETWEEN $2::text::date AND $3::text::date ORDER BY date,start_time,id",&[&ctx.actor_id(),&from,&to]).map_err(Self::database_error)?.iter().map(|r|r.get(0)).collect();
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"items":items}))
    }
    pub fn save_personal_work_block(
        &mut self,
        ctx: &AuthContext,
        id: Option<&str>,
        input: &Value,
        remove: bool,
    ) -> Result<Value, PlatformError> {
        let date = Policy::date(input, "date")?;
        let title = DomainText::required(Policy::text(input, "title", 200)?, 200)?;
        let start = Policy::text(input, "start_time", 5)?;
        let end = Policy::text(input, "end_time", 5)?;
        let parse = |v: &str| -> Result<i32, PlatformError> {
            let b = v.as_bytes();
            if b.len() != 5
                || b[2] != b':'
                || !b
                    .iter()
                    .enumerate()
                    .all(|(i, c)| i == 2 || c.is_ascii_digit())
            {
                return Err(PlatformError::Validation("invalid work block time"));
            }
            let h = v[..2]
                .parse::<i32>()
                .map_err(|_| PlatformError::Validation("invalid work block time"))?;
            let m = v[3..]
                .parse::<i32>()
                .map_err(|_| PlatformError::Validation("invalid work block time"))?;
            if h > 23 || m > 59 {
                return Err(PlatformError::Validation("invalid work block time"));
            }
            Ok(h * 60 + m)
        };
        let minutes = parse(end)? - parse(start)?;
        if minutes <= 0 {
            return Err(PlatformError::Validation("end time must follow start time"));
        }
        for field in ["task_id", "overtime_request_id", "note"] {
            if input
                .get(field)
                .is_some_and(|value| !value.is_null() && !value.is_string())
            {
                return Err(PlatformError::Validation("invalid work block field type"));
            }
        }
        let task = input["task_id"].as_str().filter(|s| !s.is_empty());
        let overtime = input["overtime_request_id"]
            .as_str()
            .filter(|s| !s.is_empty());
        let note = input["note"]
            .as_str()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());
        if note.is_some_and(|s| s.len() > 4000) {
            return Err(PlatformError::Validation(
                "work note exceeds 4000 characters",
            ));
        }
        if input.as_object().is_none_or(|o| {
            o.keys().any(|k| {
                !matches!(
                    k.as_str(),
                    "date"
                        | "title"
                        | "start_time"
                        | "end_time"
                        | "task_id"
                        | "overtime_request_id"
                        | "note"
                        | "expected_revision"
                )
            })
        }) {
            return Err(PlatformError::Validation("invalid work block field"));
        }
        let request = self.request_context.clone();
        let op = if remove {
            "calendar.work_block.remove"
        } else if id.is_some() {
            "calendar.work_block.change"
        } else {
            "calendar.work_block.create"
        };
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
            &[&format!("work-block:{}", ctx.actor_id())],
        )
        .map_err(Self::database_error)?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            op,
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        let previous: Option<Value> = if let Some(id) = id {
            Some(tx.query_opt("SELECT to_jsonb(w) FROM task_work_logs w WHERE id=$1 AND actor_id=$2 AND title IS NOT NULL AND removed_at IS NULL FOR UPDATE",&[&id,&ctx.actor_id()]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?.get(0))
        } else {
            None
        };
        if let Some(v) = &previous
            && v["revision"].as_i64() != input["expected_revision"].as_i64()
        {
            return Err(PlatformError::Conflict(
                "work block changed; reload before editing",
            ));
        }
        let resource = if let Some(task) = task {
            let t = TaskScopeAuthority::task(&mut tx, ctx, task, Capability::TasksWrite)?;
            if t.archived && !remove {
                return Err(PlatformError::Conflict("task is archived"));
            }
            Some(t)
        } else {
            None
        };
        if !remove {
            let overlap:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM task_work_logs WHERE actor_id=$1 AND date=$2::text::date AND title IS NOT NULL AND removed_at IS NULL AND ($3::text IS NULL OR id<>$3) AND start_time<$5 AND end_time>$4)",&[&ctx.actor_id(),&date,&id,&start,&end]).map_err(Self::database_error)?.get(0);
            if overlap {
                return Err(PlatformError::Conflict(
                    "work block overlaps another personal work block",
                ));
            }
            if let Some(ot) = overtime {
                let approved:i32=tx.query_opt("SELECT minutes FROM labor_overtime_requests WHERE id=$1 AND actor_id=$2 AND status='approved' AND $3::text::date BETWEEN date AND date+CASE WHEN next_day THEN 1 ELSE 0 END FOR UPDATE",&[&ot,&ctx.actor_id(),&date]).map_err(Self::database_error)?.ok_or(PlatformError::Validation("overtime must be approved for this actor and work date"))?.get(0);
                let used:i64=tx.query_one("SELECT COALESCE(sum(minutes),0)::bigint FROM task_work_logs WHERE overtime_request_id=$1 AND removed_at IS NULL AND ($2::text IS NULL OR id<>$2)",&[&ot,&id]).map_err(Self::database_error)?.get(0);
                if used + i64::from(minutes) > i64::from(approved) {
                    return Err(PlatformError::Validation(
                        "work log exceeds approved overtime minutes",
                    ));
                }
            }
        }
        let owned = match id {
            Some(v) => v.to_owned(),
            None => SecretGenerator::id("twl")?,
        };
        let value: Value = if remove {
            tx.query_one("UPDATE task_work_logs w SET removed_at=clock_timestamp(),revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(w)",&[&owned]).map_err(Self::database_error)?.get(0)
        } else if id.is_some() {
            tx.query_one("UPDATE task_work_logs w SET title=$2,date=$3::text::date,start_time=$4,end_time=$5,minutes=$6,task_id=$7,overtime_request_id=$8,note=$9,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(w)",&[&owned,&title,&date,&start,&end,&minutes,&task,&overtime,&note]).map_err(Self::database_error)?.get(0)
        } else {
            tx.query_one("INSERT INTO task_work_logs AS w(id,actor_id,title,date,start_time,end_time,minutes,task_id,overtime_request_id,note) VALUES($1,$2,$3,$4::text::date,$5,$6,$7,$8,$9,$10) RETURNING to_jsonb(w)",&[&owned,&ctx.actor_id(),&title,&date,&start,&end,&minutes,&task,&overtime,&note]).map_err(Self::database_error)?.get(0)
        };
        // Linked entries share the task ledger and retain an auditable task event.
        if let Some(t) = resource {
            TaskHistoryWriter::event(
                &mut tx,
                ctx,
                &t,
                op,
                json!({"before_work_log":previous,"work_log":value}),
            )?;
        }
        Policy::finish(&mut tx, ctx, op, Some(&owned), request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
