use crate::{
    PlatformStore, labor_calendar_policy::LaborCalendarPolicy as Policy,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn labor_calendar_settings(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        let value = tx
            .query_one(
                "SELECT to_jsonb(s)-'singleton' FROM labor_calendar_settings s WHERE singleton",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn labor_calendar(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        let admin = Policy::admin(&mut tx, ctx.actor_id())?;
        let global = Policy::global_approver(&mut tx, ctx.actor_id())?;
        let can_request = Policy::member(&mut tx, ctx.actor_id()).is_ok();
        let can_propose = Policy::developer(&mut tx, ctx.actor_id()).is_ok() || admin;
        let can_create_activity = Policy::activity_editor(&mut tx, ctx.actor_id())?;
        let settings: Value = tx
            .query_one(
                "SELECT to_jsonb(s)-'singleton' FROM labor_calendar_settings s WHERE singleton",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        let mut members = Vec::new();
        let mut visible = Vec::<String>::new();
        for row in tx.query("SELECT a.id,a.name,l.approver_id,COALESCE(l.revision,0) FROM actors a JOIN actor_roles r ON r.actor_id=a.id LEFT JOIN labor_approvers l ON l.actor_id=a.id WHERE a.active AND a.kind='human' ORDER BY a.name,a.id",&[]).map_err(Self::database_error)?{
   let id:String=row.get(0);if admin || id==ctx.actor_id() || Policy::may_decide(&mut tx,ctx.actor_id(),&id)?{visible.push(id.clone());members.push(json!({"id":id,"name":row.get::<_,String>(1),"approver_id":row.get::<_,Option<String>>(2),"approver_revision":row.get::<_,i64>(3)}))}
  }
        let mut requests = Vec::new();
        for row in tx.query("SELECT to_jsonb(r)||jsonb_build_object('actor_name',a.name,'decided_by_name',d.name) FROM labor_leave_requests r JOIN actors a ON a.id=r.actor_id LEFT JOIN actors d ON d.id=r.decided_by WHERE r.actor_id=ANY($1) OR r.status='approved' ORDER BY r.created_at DESC,r.id",&[&visible]).map_err(Self::database_error)?{let mut v:Value=row.get(0);if !visible.iter().any(|id| v["actor_id"].as_str()==Some(id.as_str())) { v["reason"]=json!("");v["decision_reason"]=json!("");v["decided_by"]=Value::Null;v["decided_by_name"]=Value::Null; }v["can_decide"]=json!(v["status"]=="pending" && crate::labor_request_approval_authority::may_decide_labor_request(&mut tx,ctx.actor_id(),v["actor_id"].as_str().unwrap_or_default(),v["kind"].as_str().unwrap_or_default())?);requests.push(v)}
        let mut holidays = Vec::new();
        for row in tx.query("SELECT to_jsonb(h)||jsonb_build_object('decided_by_name',d.name) FROM labor_holidays h LEFT JOIN actors d ON d.id=h.decided_by WHERE h.status='approved' OR h.proposed_by=$1 OR $2 ORDER BY h.date,h.id",&[&ctx.actor_id(),&global]).map_err(Self::database_error)?{let mut v:Value=row.get(0);v["can_decide"]=json!(global && v["status"]=="pending" && v["proposed_by"]!=ctx.actor_id());holidays.push(v)}
        let approvers: Vec<Value> = if admin {
            tx.query("SELECT jsonb_build_object('id',a.id,'name',a.name) FROM actors a JOIN actor_roles r ON r.actor_id=a.id WHERE a.active AND a.kind='human' AND r.role IN ('pmo','manager','technical_lead') ORDER BY a.name,a.id",&[]).map_err(Self::database_error)?.iter().map(|r|r.get(0)).collect()
        } else {
            vec![]
        };
        tx.commit().map_err(Self::database_error)?;
        Ok(
            json!({"settings":settings,"holidays":holidays,"requests":requests,"members":members,"approvers":approvers,"can_manage":admin,"can_request":can_request,"can_propose_holiday":can_propose,"can_create_activity":can_create_activity}),
        )
    }
    pub fn change_labor_calendar(
        &mut self,
        ctx: &AuthContext,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        let revision = input["expected_revision"]
            .as_i64()
            .ok_or(PlatformError::Validation("expected revision required"))?;
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        if !Policy::admin(&mut tx, ctx.actor_id())? {
            return Err(PlatformError::Forbidden);
        }
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "labor.calendar.change",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        let current:Value=tx.query_one("SELECT to_jsonb(s)-'singleton' FROM labor_calendar_settings s WHERE singleton FOR UPDATE",&[]).map_err(Self::database_error)?.get(0);
        if current["revision"].as_i64() != Some(revision) {
            return Err(PlatformError::Conflict("labor calendar changed"));
        }
        if input
            .get("country")
            .is_some_and(|value| value != &current["country"])
        {
            return Err(PlatformError::Validation(
                "configure country in organization settings",
            ));
        }
        let merged = crate::working_hours_policy::WorkingHoursPolicy::settings(&current, input)?;
        let timezone = Policy::text(&merged, "timezone", 128)?;
        let country = Policy::country(&merged["country"])?;
        let starts = Policy::date(&merged, "starts_on")?;
        let minutes = merged["daily_minutes"]
            .as_i64()
            .ok_or(PlatformError::Validation("invalid daily minutes"))?
            as i32;
        let pause = merged["break_minutes"]
            .as_i64()
            .ok_or(PlatformError::Validation("invalid break minutes"))? as i32;
        let start = Policy::text(&merged, "work_start", 5)?;
        let end = Policy::text(&merged, "work_end", 5)?;
        let days: Vec<i32> = merged["workdays"]
            .as_array()
            .ok_or(PlatformError::Validation("invalid workdays"))?
            .iter()
            .map(|v| {
                v.as_i64()
                    .filter(|d| (1..=7).contains(d))
                    .map(|d| d as i32)
                    .ok_or(PlatformError::Validation("invalid workday"))
            })
            .collect::<Result<_, _>>()?;
        if days.is_empty() || days.iter().enumerate().any(|(i, d)| days[..i].contains(d)) {
            return Err(PlatformError::Validation("invalid workdays"));
        }
        if tx
            .query_opt(
                "SELECT name FROM pg_timezone_names WHERE name=$1",
                &[&timezone],
            )
            .map_err(Self::database_error)?
            .is_none()
        {
            return Err(PlatformError::Validation("invalid timezone"));
        }
        let v:Value=tx.query_one("UPDATE labor_calendar_settings s SET timezone=$1,country=$2,workdays=$3,daily_minutes=$4,starts_on=$5::text::date,work_start=$6,work_end=$7,break_minutes=$8,revision=revision+1 WHERE singleton RETURNING to_jsonb(s)-'singleton'",&[&timezone,&country,&days,&minutes,&starts,&start,&end,&pause]).map_err(Self::database_error)?.get(0);
        Policy::finish(
            &mut tx,
            ctx,
            "labor.calendar.change",
            None,
            request.as_ref(),
            &v,
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(v)
    }
    pub fn assign_labor_approver(
        &mut self,
        ctx: &AuthContext,
        actor: &str,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        let approver = if input["approver_id"].is_null() {
            None
        } else {
            Some(Policy::text(input, "approver_id", 200)?)
        };
        let revision = input["expected_revision"]
            .as_i64()
            .ok_or(PlatformError::Validation("expected revision required"))?;
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        if !Policy::admin(&mut tx, ctx.actor_id())? {
            return Err(PlatformError::Forbidden);
        }
        Policy::member(&mut tx, actor)?;
        if let Some(approver) = approver
            && (approver == actor || !Policy::eligible(&mut tx, approver, actor)?)
        {
            return Err(PlatformError::Forbidden);
        }
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "labor.approver.assign",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        tx.query_one(
            "SELECT singleton FROM labor_calendar_settings WHERE singleton FOR UPDATE",
            &[],
        )
        .map_err(Self::database_error)?;
        let actual = tx
            .query_opt(
                "SELECT revision FROM labor_approvers WHERE actor_id=$1",
                &[&actor],
            )
            .map_err(Self::database_error)?
            .map(|r| r.get::<_, i64>(0))
            .unwrap_or(0);
        if actual != revision {
            return Err(PlatformError::Conflict("labor approver changed"));
        }
        let v:Value=tx.query_one("INSERT INTO labor_approvers(actor_id,approver_id) VALUES($1,$2) ON CONFLICT(actor_id) DO UPDATE SET approver_id=EXCLUDED.approver_id,revision=labor_approvers.revision+1 RETURNING to_jsonb(labor_approvers)",&[&actor,&approver]).map_err(Self::database_error)?.get(0);
        Policy::finish(
            &mut tx,
            ctx,
            "labor.approver.assign",
            Some(actor),
            request.as_ref(),
            &v,
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(v)
    }
}
