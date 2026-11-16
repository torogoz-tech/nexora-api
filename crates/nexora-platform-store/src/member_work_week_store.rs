use crate::PlatformStore;
use nexora_platform::{AuthContext, PlatformError};
impl PlatformStore {
    pub fn member_work_week(
        &mut self,
        ctx: &AuthContext,
        member: &str,
        date: Option<&str>,
    ) -> Result<serde_json::Value, PlatformError> {
        let mut tx = self.client.transaction().map_err(Self::database_error)?;
        crate::labor_calendar_policy::LaborCalendarPolicy::session(&mut tx, ctx)?;
        tx.query_opt("SELECT a.id FROM actors a JOIN users u ON u.actor_id=a.id JOIN actor_roles r ON r.actor_id=a.id WHERE a.id=$1 AND r.role IN ('developer','technical_lead') FOR SHARE OF a,u,r",&[&member]).map_err(Self::database_error)?.ok_or(PlatformError::NotFound)?;
        if let Some(date) = date {
            crate::labor_calendar_policy::LaborCalendarPolicy::date(
                &serde_json::json!({"date":date}),
                "date",
            )?;
        }
        let value = tx
            .query_one(include_str!("member_work_week.sql"), &[&member, &date])
            .map_err(Self::database_error)?
            .get(0);
        tx.commit().map_err(Self::database_error)?;
        Ok(value)
    }
}
