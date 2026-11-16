use crate::{PlatformStore, labor_calendar_policy::LaborCalendarPolicy as Policy};
use nexora_platform::PlatformError;
pub(crate) fn overtime_approval_authority(
    tx: &mut postgres::Transaction<'_>,
    reviewer: &str,
    target: &str,
) -> Result<bool, PlatformError> {
    if reviewer == target {
        return Ok(false);
    }
    Policy::member(tx, target)?;
    let role: Option<String> = tx
        .query_opt(
            "SELECT role FROM actor_roles WHERE actor_id=$1 FOR SHARE",
            &[&target],
        )
        .map_err(PlatformStore::database_error)?
        .map(|r| r.get(0));
    if matches!(role.as_deref(), Some("developer" | "technical_lead")) {
        return Policy::may_decide(tx, reviewer, target);
    }
    Ok(tx.query_one("SELECT EXISTS(SELECT 1 FROM actor_roles r JOIN actors a ON a.id=r.actor_id WHERE a.active AND r.actor_id=$1 AND r.role IN ('manager','super_admin'))",&[&reviewer]).map_err(PlatformStore::database_error)?.get(0))
}
