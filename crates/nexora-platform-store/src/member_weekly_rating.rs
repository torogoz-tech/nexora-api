use crate::{PlatformStore, labor_calendar_policy::LaborCalendarPolicy};
use nexora_platform::{AuthContext, PlatformError};
use serde_json::Value;

impl PlatformStore {
    pub fn own_weekly_rating(&mut self, ctx: &AuthContext) -> Result<Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        LaborCalendarPolicy::session(&mut tx, ctx)?;
        Self::rating_target(&mut tx, ctx.actor_id())?;
        let value = tx
            .query_one(include_str!("member_weekly_rating.sql"), &[&ctx.actor_id()])
            .map_err(Self::database_error)?
            .get(0);
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
