use crate::{
    PlatformStore, SecretGenerator, labor_calendar_policy::LaborCalendarPolicy as Policy,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::Value;
impl PlatformStore {
    pub fn create_labor_leave_request(
        &mut self,
        ctx: &AuthContext,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        let kind = Policy::text(input, "kind", 20)?;
        if !matches!(kind, "vacation" | "leave") {
            return Err(PlatformError::Validation("invalid leave kind"));
        }
        let start = Policy::date(input, "start_date")?;
        let end = Policy::date(input, "end_date")?;
        if end < start {
            return Err(PlatformError::Validation("invalid leave dates"));
        }
        let reason = crate::labor_leave_reason::labor_leave_reason(input, kind)?;
        let minutes = if input["minutes_per_day"].is_null() {
            None
        } else {
            Some(
                input["minutes_per_day"]
                    .as_i64()
                    .filter(|v| (1..=1440).contains(v))
                    .ok_or(PlatformError::Validation("invalid leave minutes"))?
                    as i32,
            )
        };
        if kind == "vacation" && minutes.is_some() {
            return Err(PlatformError::Validation("vacation must use full days"));
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        Policy::member(&mut tx, ctx.actor_id())?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "labor.request.create",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        if tx
            .query_one(
                "SELECT ($2::text::date-$1::text::date)>365",
                &[&start, &end],
            )
            .map_err(Self::database_error)?
            .get::<_, bool>(0)
        {
            return Err(PlatformError::Validation("leave range exceeds 366 days"));
        }
        let daily: i32 = tx
            .query_one(
                "SELECT daily_minutes FROM labor_calendar_settings WHERE singleton FOR UPDATE",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        if minutes.is_some_and(|m| m > daily) {
            return Err(PlatformError::Validation("leave minutes exceed workday"));
        }
        if tx.query_one("SELECT EXISTS(SELECT 1 FROM labor_leave_requests WHERE actor_id=$1 AND status IN ('pending','approved') AND start_date<=$3::text::date AND end_date>=$2::text::date)",&[&ctx.actor_id(),&start,&end]).map_err(Self::database_error)?.get::<_,bool>(0){return Err(PlatformError::Conflict("leave dates overlap an existing request"))}
        let id = SecretGenerator::id("lvr")?;
        let v=tx.query_one("INSERT INTO labor_leave_requests(id,actor_id,kind,start_date,end_date,minutes_per_day,reason) VALUES($1,$2,$3,$4::text::date,$5::text::date,$6,$7) RETURNING to_jsonb(labor_leave_requests)",&[&id,&ctx.actor_id(),&kind,&start,&end,&minutes,&reason]).map_err(Self::database_error)?.get(0);
        Policy::finish(
            &mut tx,
            ctx,
            "labor.request.create",
            Some(&id),
            request.as_ref(),
            &v,
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(v)
    }
    pub fn propose_labor_holiday(
        &mut self,
        ctx: &AuthContext,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        let name = Policy::text(input, "name", 160)?;
        let date = Policy::date(input, "date")?;
        let reason = match input.get("reason") {
            None | Some(Value::Null) => "",
            Some(Value::String(value)) => value.as_str(),
            _ => return Err(PlatformError::Validation("invalid holiday reason")),
        };
        if reason.len() > 4000 || reason.chars().any(|c| c.is_control() && c != '\n') {
            return Err(PlatformError::Validation("invalid holiday reason"));
        }
        let kind = input
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("holiday");
        if !["holiday", "public_holiday"].contains(&kind) {
            return Err(PlatformError::Validation("invalid holiday kind"));
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        // Administrators can manually register a holiday; developers submit proposals.
        if !Policy::admin(&mut tx, ctx.actor_id())? {
            Policy::developer(&mut tx, ctx.actor_id())?
        }
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "labor.holiday.propose",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        let id = SecretGenerator::id("lhd")?;
        let admin = Policy::admin(&mut tx, ctx.actor_id())?;
        tx.query_one(
            "SELECT singleton FROM labor_calendar_settings WHERE singleton FOR UPDATE",
            &[],
        )
        .map_err(Self::database_error)?;
        let country: Option<String> = tx
            .query_one(
                "SELECT country FROM labor_calendar_settings WHERE singleton",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        if tx.query_one("SELECT EXISTS(SELECT 1 FROM labor_holidays WHERE date=$1::text::date AND country IS NOT DISTINCT FROM $2 AND status IN ('pending','approved'))",&[&date,&country]).map_err(Self::database_error)?.get::<_,bool>(0){return Err(PlatformError::Conflict("holiday already proposed or approved"))}
        let status = if admin { "approved" } else { "pending" };
        let v=tx.query_one("INSERT INTO labor_holidays(id,name,date,country,proposed_by,reason,status,decided_by,decided_at,kind) VALUES($1,$2,$3::text::date,$4,$5,$6,$7,CASE WHEN $8 THEN $5 ELSE NULL END,CASE WHEN $8 THEN clock_timestamp() ELSE NULL END,$9) RETURNING to_jsonb(labor_holidays)",&[&id,&name,&date,&country,&ctx.actor_id(),&reason,&status,&admin,&kind]).map_err(Self::database_error)?.get(0);
        Policy::finish(
            &mut tx,
            ctx,
            "labor.holiday.propose",
            Some(&id),
            request.as_ref(),
            &v,
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(v)
    }
    pub fn decide_labor_request(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        self.decide_labor_calendar_item(ctx, id, input, false)
    }
    pub fn decide_labor_holiday(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        self.decide_labor_calendar_item(ctx, id, input, true)
    }
    fn decide_labor_calendar_item(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &Value,
        holiday: bool,
    ) -> Result<Value, PlatformError> {
        let approved = input["approved"]
            .as_bool()
            .ok_or(PlatformError::Validation("approval decision required"))?;
        let reason = Policy::text(input, "reason", 4000)?;
        let op = if holiday {
            "labor.holiday.decide"
        } else {
            "labor.request.decide"
        };
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        tx.query_one(
            "SELECT singleton FROM labor_calendar_settings WHERE singleton FOR UPDATE",
            &[],
        )
        .map_err(Self::database_error)?;
        let sql = if holiday {
            "SELECT proposed_by,status FROM labor_holidays WHERE id=$1 FOR UPDATE"
        } else {
            "SELECT actor_id,status,kind FROM labor_leave_requests WHERE id=$1 FOR UPDATE"
        };
        let row = tx
            .query_opt(sql, &[&id])
            .map_err(Self::database_error)?
            .ok_or(PlatformError::NotFound)?;
        let actor: String = row.get(0);
        let allowed = if holiday {
            actor != ctx.actor_id() && Policy::global_approver(&mut tx, ctx.actor_id())?
        } else {
            Policy::member(&mut tx, &actor)?;
            crate::labor_request_approval_authority::may_decide_labor_request(
                &mut tx,
                ctx.actor_id(),
                &actor,
                row.get::<_, &str>(2),
            )?
        };
        if !allowed {
            return Err(PlatformError::Forbidden);
        }
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
        if row.get::<_, String>(1) != "pending" {
            return Err(PlatformError::Conflict("labor decision already recorded"));
        }
        let status = if approved { "approved" } else { "rejected" };
        let sql = if holiday {
            "UPDATE labor_holidays SET status=$2,decision_reason=$3,decided_by=$4,decided_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(labor_holidays)"
        } else {
            "UPDATE labor_leave_requests SET status=$2,decision_reason=$3,decided_by=$4,decided_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(labor_leave_requests)"
        };
        let v = tx
            .query_one(sql, &[&id, &status, &reason, &ctx.actor_id()])
            .map_err(Self::database_error)?
            .get(0);
        Policy::finish(&mut tx, ctx, op, Some(id), request.as_ref(), &v)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(v)
    }
}
