use crate::{PlatformStore, labor_calendar_policy::LaborCalendarPolicy};
use nexora_platform::PlatformError;
use postgres::Transaction;

pub(crate) fn may_decide_labor_request(
    tx: &mut Transaction<'_>,
    reviewer: &str,
    target: &str,
    kind: &str,
) -> Result<bool, PlatformError> {
    if kind != "vacation" {
        return LaborCalendarPolicy::may_decide(tx, reviewer, target);
    }
    if reviewer == target {
        return Ok(false);
    }
    Ok(tx.query_one(
        "SELECT EXISTS(SELECT 1 FROM actor_roles r JOIN actors a ON a.id=r.actor_id WHERE r.actor_id=$1 AND a.active AND a.kind='human' AND r.role IN ('manager','super_admin')) AND EXISTS(SELECT 1 FROM actors WHERE id=$2 AND active AND kind='human')",
        &[&reviewer, &target],
    ).map_err(PlatformStore::database_error)?.get(0))
}
