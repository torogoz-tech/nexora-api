use crate::{
    PlatformStore, SecretGenerator, labor_calendar_policy::LaborCalendarPolicy as Policy,
    receipt_writer::ReceiptWriter, working_hours_policy::WorkingHoursPolicy,
};
use nexora_platform::*;
use serde_json::{Value, json};
impl PlatformStore {
    pub fn overtime(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        let admin = Policy::admin(&mut tx, ctx.actor_id())?;
        let can_request = Policy::member(&mut tx, ctx.actor_id()).is_ok();
        let mut visible = Vec::<String>::new();
        for row in tx.query("SELECT a.id FROM actors a JOIN actor_roles r ON r.actor_id=a.id WHERE a.active AND a.kind='human' ORDER BY a.id",&[]).map_err(Self::database_error)?{let actor:String=row.get(0);if actor==ctx.actor_id() || admin || crate::overtime_approval_authority::overtime_approval_authority(&mut tx,ctx.actor_id(),&actor)?{visible.push(actor)}}
        let mut items = Vec::new();
        let mut total = 0_i64;
        for row in tx.query("SELECT to_jsonb(o)||jsonb_build_object('actor_name',a.name,'decided_by_name',d.name) FROM labor_overtime_requests o JOIN actors a ON a.id=o.actor_id LEFT JOIN actors d ON d.id=o.decided_by WHERE o.actor_id=ANY($1) ORDER BY o.date DESC,o.start_time DESC,o.id",&[&visible]).map_err(Self::database_error)?{let mut v:Value=row.get(0);if !admin && v["actor_id"]!=ctx.actor_id() && !Self::overtime_may_decide(&mut tx,ctx,v["id"].as_str().unwrap_or_default(),v["actor_id"].as_str().unwrap_or_default())?{continue}v["can_decide"]=json!(v["status"]=="pending" && Self::overtime_may_decide(&mut tx,ctx,v["id"].as_str().unwrap_or_default(),v["actor_id"].as_str().unwrap_or_default())?);if v["status"]=="approved"{total+=v["minutes"].as_i64().unwrap_or(0)}let evidence=crate::overtime_evidence_store::OvertimeEvidenceStore::projection(&mut tx,ctx,v["id"].as_str().unwrap_or_default())?;v["tasks"]=evidence["tasks"].clone();v["commits"]=evidence["commits"].clone();v["can_attach_evidence"]=evidence["can_attach_evidence"].clone();v["evidence_unavailable"]=evidence["evidence_unavailable"].clone();v["evidence_unavailable_count"]=evidence["evidence_unavailable_count"].clone();items.push(v)}
        tx.commit().map_err(Self::database_error)?;
        Ok(
            json!({"items":items,"can_request":can_request,"can_manage":admin,"total_approved_minutes":total}),
        )
    }
    pub fn create_overtime_request(
        &mut self,
        ctx: &AuthContext,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        if input.as_object().is_none_or(|o| {
            o.keys().any(|k| {
                !matches!(
                    k.as_str(),
                    "date" | "start_time" | "end_time" | "next_day" | "reason" | "task_ids"
                )
            })
        }) {
            return Err(PlatformError::Validation("invalid overtime request"));
        }
        let date = Policy::date(input, "date")?;
        if date.as_str() < "2000-01-01" || date.as_str() > "9998-12-31" {
            return Err(PlatformError::Validation("invalid overtime date"));
        }
        let start = WorkingHoursPolicy::clock(&input["start_time"])?;
        let end = WorkingHoursPolicy::clock(&input["end_time"])?;
        let next = input["next_day"]
            .as_bool()
            .ok_or(PlatformError::Validation("next day must be specified"))?;
        let minutes = end - start + if next { 1440 } else { 0 };
        if !(1..=1440).contains(&minutes) {
            return Err(PlatformError::Validation(
                "overtime duration must be between 1 minute and 24 hours",
            ));
        }
        let reason = input
            .get("reason")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Tareas vinculadas")
            .to_owned();
        let start_time = input["start_time"].as_str().unwrap_or_default();
        let end_time = input["end_time"].as_str().unwrap_or_default();
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        Policy::member(&mut tx, ctx.actor_id())?;
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "overtime.request.create",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        let timezone: String = tx
            .query_one(
                "SELECT timezone FROM labor_calendar_settings WHERE singleton FOR UPDATE",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        let valid_window:bool=tx.query_one("SELECT (($1::text::date+$3::text::time+CASE WHEN $4 THEN interval '1 day' ELSE interval '0 day' END) AT TIME ZONE $5)>(($1::text::date+$2::text::time) AT TIME ZONE $5) AND ((($1::text::date+$2::text::time) AT TIME ZONE $5) AT TIME ZONE $5)=($1::text::date+$2::text::time) AND ((($1::text::date+$3::text::time+CASE WHEN $4 THEN interval '1 day' ELSE interval '0 day' END) AT TIME ZONE $5) AT TIME ZONE $5)=($1::text::date+$3::text::time+CASE WHEN $4 THEN interval '1 day' ELSE interval '0 day' END)",&[&date,&start_time,&end_time,&next,&timezone]).map_err(Self::database_error)?.get(0);
        if !valid_window {
            return Err(PlatformError::Validation(
                "overtime window crosses a nonexistent local time",
            ));
        }
        let overlap:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM labor_overtime_requests o WHERE o.actor_id=$1 AND o.status IN ('pending','approved') AND tstzrange((o.date+o.start_time::time) AT TIME ZONE o.timezone,(o.date+o.end_time::time+CASE WHEN o.next_day THEN interval '1 day' ELSE interval '0 day' END) AT TIME ZONE o.timezone,'[)') && tstzrange(($2::text::date+$3::text::time) AT TIME ZONE $6,($2::text::date+$4::text::time+CASE WHEN $5 THEN interval '1 day' ELSE interval '0 day' END) AT TIME ZONE $6,'[)'))",&[&ctx.actor_id(),&date,&start_time,&end_time,&next,&timezone]).map_err(Self::database_error)?.get(0);
        if overlap {
            return Err(PlatformError::Conflict(
                "overtime overlaps an existing request",
            ));
        }
        let id = SecretGenerator::id("ovt")?;
        let v=tx.query_one("INSERT INTO labor_overtime_requests(id,actor_id,date,start_time,end_time,next_day,minutes,reason,timezone) VALUES($1,$2,$3::text::date,$4,$5,$6,$7,$8,$9) RETURNING to_jsonb(labor_overtime_requests)",&[&id,&ctx.actor_id(),&date,&start_time,&end_time,&next,&minutes,&reason,&timezone]).map_err(Self::database_error)?.get(0);
        let task_ids =
            crate::overtime_evidence_store::OvertimeEvidenceStore::parse_task_ids(input)?;
        crate::overtime_evidence_store::OvertimeEvidenceStore::bind_tasks(
            &mut tx, ctx, &id, &task_ids,
        )?;
        Policy::finish(
            &mut tx,
            ctx,
            "overtime.request.create",
            Some(&id),
            request.as_ref(),
            &v,
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(v)
    }
    pub fn decide_overtime_request(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        if input.as_object().is_none_or(|o| {
            o.keys()
                .any(|k| !matches!(k.as_str(), "approved" | "reason"))
        }) {
            return Err(PlatformError::Validation("invalid overtime decision"));
        }
        let approved = input["approved"]
            .as_bool()
            .ok_or(PlatformError::Validation("approval decision required"))?;
        let reason = Policy::text(input, "reason", 4000)?;
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        tx.query_one(
            "SELECT singleton FROM labor_calendar_settings WHERE singleton FOR UPDATE",
            &[],
        )
        .map_err(Self::database_error)?;
        let row = tx
            .query_opt(
                "SELECT actor_id,status FROM labor_overtime_requests WHERE id=$1 FOR UPDATE",
                &[&id],
            )
            .map_err(Self::database_error)?
            .ok_or(PlatformError::NotFound)?;
        let actor: String = row.get(0);
        Policy::member(&mut tx, &actor)?;
        if !Self::overtime_may_decide(&mut tx, ctx, id, &actor)? {
            return Err(PlatformError::Forbidden);
        }
        if let Some(v) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "overtime.request.decide",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        if row.get::<_, String>(1) != "pending" {
            return Err(PlatformError::Conflict(
                "overtime decision already recorded",
            ));
        }
        let status = if approved { "approved" } else { "rejected" };
        let v=tx.query_one("UPDATE labor_overtime_requests SET status=$2,decision_reason=$3,decided_by=$4,decided_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(labor_overtime_requests)",&[&id,&status,&reason,&ctx.actor_id()]).map_err(Self::database_error)?.get(0);
        Policy::finish(
            &mut tx,
            ctx,
            "overtime.request.decide",
            Some(id),
            request.as_ref(),
            &v,
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(v)
    }
    fn overtime_may_decide(
        tx: &mut postgres::Transaction<'_>,
        ctx: &AuthContext,
        id: &str,
        actor: &str,
    ) -> Result<bool, PlatformError> {
        if !crate::overtime_approval_authority::overtime_approval_authority(
            tx,
            ctx.actor_id(),
            actor,
        )? {
            return Ok(false);
        }
        if Policy::admin(tx, ctx.actor_id())? {
            return Ok(true);
        }
        let projects =
            crate::overtime_evidence_store::OvertimeEvidenceStore::request_projects(tx, id)?;
        for project in projects {
            let covered:bool=tx.query_one("SELECT EXISTS(SELECT 1 FROM actor_roles r WHERE r.actor_id=$1 AND (r.all_projects OR EXISTS(SELECT 1 FROM member_role_projects mp WHERE mp.actor_id=r.actor_id AND mp.project_id=$2)))",&[&ctx.actor_id(),&project]).map_err(Self::database_error)?.get(0);
            if !covered {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
