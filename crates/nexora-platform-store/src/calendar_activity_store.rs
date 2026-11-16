use crate::{
    PlatformStore, SecretGenerator, calendar_activity_input::CalendarActivityInput,
    labor_calendar_policy::LaborCalendarPolicy as Policy, receipt_writer::ReceiptWriter,
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn calendar_activity(
        &mut self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        let admin = Policy::admin(&mut tx, ctx.actor_id())?;
        let editor = Policy::activity_editor(&mut tx, ctx.actor_id())?;
        let mut value:Value=tx.query_opt("SELECT to_jsonb(e)||jsonb_build_object('creator_name',a.name) FROM team_calendar_activities e JOIN actors a ON a.id=e.created_by WHERE e.id=$1 AND e.status='active'",&[&id]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        let revision = value["revision"]
            .as_i64()
            .ok_or(PlatformError::Unavailable)?;
        let participants:Vec<Value>=tx.query("SELECT r.actor_id,a.name,r.response FROM calendar_activity_attendance r JOIN actors a ON a.id=r.actor_id WHERE r.activity_id=$1 AND r.activity_revision=$2 ORDER BY lower(a.name),a.id",&[&id,&revision]).map_err(Self::database_error)?.iter().map(|r|json!({"actor_id":r.get::<_,String>(0),"name":r.get::<_,String>(1),"response":r.get::<_,String>(2)})).collect();
        value["attending_count"] = json!(
            participants
                .iter()
                .filter(|p| p["response"] == "attending")
                .count()
        );
        value["declined_count"] = json!(
            participants
                .iter()
                .filter(|p| p["response"] == "declined")
                .count()
        );
        value["my_attendance"] = participants
            .iter()
            .find(|p| p["actor_id"] == ctx.actor_id())
            .map(|p| p["response"].clone())
            .unwrap_or(Value::Null);
        value["participants"] = json!(participants);
        value["can_edit"] = json!(editor && (admin || value["created_by"] == ctx.actor_id()));
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }

    pub fn calendar_activities(
        &mut self,
        ctx: &AuthContext,
        from: &str,
        to: &str,
        after: Option<&str>,
        limit: u32,
    ) -> Result<Value, PlatformError> {
        let dates = json!({"from":from,"to":to});
        Policy::date(&dates, "from")?;
        Policy::date(&dates, "to")?;
        if !(1..=100).contains(&limit) || from > to || after.is_some_and(|s| s.len() > 200) {
            return Err(PlatformError::Validation("invalid calendar activity query"));
        }
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        if tx
            .query_one("SELECT $2::text::date-$1::text::date>62", &[&from, &to])
            .map_err(Self::database_error)?
            .get::<_, bool>(0)
        {
            return Err(PlatformError::Validation(
                "calendar activity range exceeds 63 days",
            ));
        }
        let admin = Policy::admin(&mut tx, ctx.actor_id())?;
        let editor = Policy::activity_editor(&mut tx, ctx.actor_id())?;
        let size = i64::from(limit) + 1;
        let rows=tx.query("SELECT to_jsonb(e)||jsonb_build_object('creator_name',a.name,'my_attendance',(SELECT response FROM calendar_activity_attendance r WHERE r.activity_id=e.id AND r.actor_id=$5 AND r.activity_revision=e.revision),'attending_count',(SELECT count(*) FROM calendar_activity_attendance r WHERE r.activity_id=e.id AND r.activity_revision=e.revision AND r.response='attending'),'declined_count',(SELECT count(*) FROM calendar_activity_attendance r WHERE r.activity_id=e.id AND r.activity_revision=e.revision AND r.response='declined')) FROM team_calendar_activities e JOIN actors a ON a.id=e.created_by WHERE e.status='active' AND e.start_date<=$2::text::date AND e.end_date>=$1::text::date AND ($3::text IS NULL OR e.id>$3) ORDER BY e.id LIMIT $4",&[&from,&to,&after,&size,&ctx.actor_id()]).map_err(Self::database_error)?;
        let more = rows.len() > limit as usize;
        let items: Vec<Value> = rows
            .into_iter()
            .take(limit as usize)
            .map(|r| {
                let mut v: Value = r.get(0);
                v["can_edit"] = json!(editor && (admin || v["created_by"] == ctx.actor_id()));
                v
            })
            .collect();
        let next = if more {
            items.last().map(|v| v["id"].clone())
        } else {
            None
        };
        tx.commit().map_err(Self::database_error)?;
        Ok(json!({"items":items,"has_more":more,"next_after":next}))
    }
    pub fn save_calendar_activity(
        &mut self,
        ctx: &AuthContext,
        id: Option<&str>,
        input: &Value,
        cancel: bool,
    ) -> Result<Value, PlatformError> {
        if cancel {
            if input
                .as_object()
                .is_none_or(|o| o.len() != 1 || !o.contains_key("expected_revision"))
            {
                return Err(PlatformError::Validation(
                    "invalid calendar activity cancellation",
                ));
            }
        } else {
            CalendarActivityInput::validate(input, id.is_some())?;
        }
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        if !Policy::activity_editor(&mut tx, ctx.actor_id())? {
            return Err(PlatformError::Forbidden);
        }
        let admin = Policy::admin(&mut tx, ctx.actor_id())?;
        let op = if cancel {
            "calendar.activity.cancel"
        } else if id.is_some() {
            "calendar.activity.change"
        } else {
            "calendar.activity.create"
        };
        if let Some(id) = id {
            let row = tx
                .query_opt(
                    "SELECT created_by FROM team_calendar_activities WHERE id=$1 FOR UPDATE",
                    &[&id],
                )
                .map_err(Self::database_error)?
                .ok_or(PlatformError::NotFound)?;
            if !admin && row.get::<_, String>(0) != ctx.actor_id() {
                return Err(PlatformError::Forbidden);
            }
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
        let owned_id = match id {
            Some(id) => id.to_owned(),
            None => SecretGenerator::id("evt")?,
        };
        if let Some(id) = id {
            let rev = input["expected_revision"]
                .as_i64()
                .ok_or(PlatformError::Validation("expected revision required"))?;
            let row = tx
                .query_one(
                    "SELECT revision,status FROM team_calendar_activities WHERE id=$1",
                    &[&id],
                )
                .map_err(Self::database_error)?;
            if row.get::<_, i64>(0) != rev || row.get::<_, String>(1) != "active" {
                return Err(PlatformError::Conflict("calendar activity changed"));
            }
        }
        let value: Value = if cancel {
            tx.query_one("UPDATE team_calendar_activities SET status='cancelled',revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(team_calendar_activities)",&[&owned_id]).map_err(Self::database_error)?.get(0)
        } else {
            let mode = input["mode"].as_str();
            let attendance_required = input["attendance_required"].as_bool().unwrap_or(false);
            let title = input["title"].as_str().unwrap().trim();
            let description = input["description"].as_str().unwrap_or("").trim();
            let location = input["location"].as_str().unwrap_or("").trim();
            let start = input["start_date"].as_str().unwrap();
            let end = input["end_date"].as_str().unwrap();
            let all_day = input["all_day"].as_bool().unwrap();
            let start_time = input["start_time"].as_str();
            let end_time = input["end_time"].as_str();
            if id.is_some() {
                tx.query_one("UPDATE team_calendar_activities SET title=$2,description=$3,location=$4,start_date=$5::text::date,end_date=$6::text::date,all_day=$7,start_time=$8,end_time=$9,mode=$10,attendance_required=$11,revision=revision+1,updated_at=clock_timestamp() WHERE id=$1 RETURNING to_jsonb(team_calendar_activities)",&[&owned_id,&title,&description,&location,&start,&end,&all_day,&start_time,&end_time,&mode,&attendance_required]).map_err(Self::database_error)?.get(0)
            } else {
                tx.query_one("INSERT INTO team_calendar_activities(id,created_by,title,description,location,start_date,end_date,all_day,start_time,end_time,mode,attendance_required) VALUES($1,$2,$3,$4,$5,$6::text::date,$7::text::date,$8,$9,$10,$11,$12) RETURNING to_jsonb(team_calendar_activities)",&[&owned_id,&ctx.actor_id(),&title,&description,&location,&start,&end,&all_day,&start_time,&end_time,&mode,&attendance_required]).map_err(Self::database_error)?.get(0)
            }
        };
        Policy::finish(&mut tx, ctx, op, Some(&owned_id), request.as_ref(), &value)?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
