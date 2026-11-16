use crate::{
    PlatformStore, labor_calendar_policy::LaborCalendarPolicy as Policy,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::{Value, json};
impl PlatformStore {
    pub fn respond_calendar_activity(
        &mut self,
        ctx: &AuthContext,
        id: &str,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        if input.as_object().is_none_or(|o| {
            o.len() != 2 || !o.contains_key("response") || !o.contains_key("expected_revision")
        }) {
            return Err(PlatformError::Validation("invalid attendance response"));
        }
        let response = input["response"]
            .as_str()
            .filter(|r| ["attending", "declined"].contains(r))
            .ok_or(PlatformError::Validation("invalid attendance response"))?;
        let revision = input["expected_revision"]
            .as_i64()
            .ok_or(PlatformError::Validation("expected revision required"))?;
        let request = self.request_context.clone();
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        Policy::member(&mut tx, ctx.actor_id())?;
        let row=tx.query_opt("SELECT revision,status,attendance_required FROM team_calendar_activities WHERE id=$1 FOR SHARE",&[&id]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        if row.get::<_, i64>(0) != revision
            || row.get::<_, String>(1) != "active"
            || !row.get::<_, bool>(2)
        {
            return Err(PlatformError::Conflict("calendar activity changed"));
        }
        if let Some(value) = ReceiptWriter::read(
            &mut tx,
            &self.namespace,
            request.as_ref(),
            ctx.actor_id(),
            "calendar.activity.attendance",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(value);
        }
        tx.execute("INSERT INTO calendar_activity_attendance(activity_id,actor_id,activity_revision,response) VALUES($1,$2,$3,$4) ON CONFLICT(activity_id,actor_id) DO UPDATE SET activity_revision=EXCLUDED.activity_revision,response=EXCLUDED.response,updated_at=clock_timestamp()",&[&id,&ctx.actor_id(),&revision,&response]).map_err(Self::database_error)?;
        let value = json!({"response":response,"activity_revision":revision});
        Policy::finish(
            &mut tx,
            ctx,
            "calendar.activity.attendance",
            Some(id),
            request.as_ref(),
            &value,
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
