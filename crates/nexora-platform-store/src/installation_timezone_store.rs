use crate::{
    PlatformStore, labor_calendar_policy::LaborCalendarPolicy as Policy,
    receipt_writer::ReceiptWriter,
};
use nexora_platform::*;
use serde_json::Value;
impl PlatformStore {
    pub fn installation_timezone(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        Policy::session(&mut tx, ctx)?;
        let value=tx.query_one("SELECT jsonb_build_object('timezone',timezone,'revision',revision) FROM labor_calendar_settings WHERE singleton",&[]).map_err(Self::database_error)?.get(0);
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
    pub fn change_installation_timezone(
        &mut self,
        ctx: &AuthContext,
        input: &Value,
    ) -> Result<Value, PlatformError> {
        let zone = Policy::text(input, "timezone", 128)?;
        let revision = input["expected_revision"]
            .as_i64()
            .filter(|v| *v > 0)
            .ok_or(PlatformError::Validation("expected revision required"))?;
        if input.as_object().is_none_or(|o| {
            o.keys()
                .any(|k| !matches!(k.as_str(), "timezone" | "expected_revision"))
        }) {
            return Err(PlatformError::Validation("invalid timezone settings"));
        }
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
            "installation.timezone.change",
        )? {
            tx.commit().map_err(Self::database_error)?;
            return Ok(v);
        }
        let actual: i64 = tx
            .query_one(
                "SELECT revision FROM labor_calendar_settings WHERE singleton FOR UPDATE",
                &[],
            )
            .map_err(Self::database_error)?
            .get(0);
        if revision != actual {
            return Err(PlatformError::Conflict("labor calendar changed"));
        }
        if tx
            .query_opt("SELECT name FROM pg_timezone_names WHERE name=$1", &[&zone])
            .map_err(Self::database_error)?
            .is_none()
        {
            return Err(PlatformError::Validation("invalid timezone"));
        }
        let v=tx.query_one("UPDATE labor_calendar_settings SET timezone=$1,revision=revision+1 WHERE singleton RETURNING jsonb_build_object('timezone',timezone,'revision',revision)",&[&zone]).map_err(Self::database_error)?.get(0);
        Policy::finish(
            &mut tx,
            ctx,
            "installation.timezone.change",
            None,
            request.as_ref(),
            &v,
        )?;
        tx.commit().map_err(Self::database_error)?;
        Ok(v)
    }
}
