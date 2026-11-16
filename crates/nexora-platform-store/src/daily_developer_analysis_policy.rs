use crate::{PlatformStore, labor_calendar_policy::LaborCalendarPolicy};
use nexora_platform::{AuthContext, PlatformError};
pub(crate) struct DailyDeveloperAnalysisPolicy;
impl DailyDeveloperAnalysisPolicy {
    pub fn authorize(
        tx: &mut postgres::Transaction<'_>,
        ctx: &AuthContext,
        target: Option<&str>,
    ) -> Result<(), PlatformError> {
        LaborCalendarPolicy::session(tx, ctx)?;
        let role: String = tx
            .query_opt(
                "SELECT role FROM actor_roles WHERE actor_id=$1",
                &[&ctx.actor_id()],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::Forbidden)?
            .get(0);
        if !Self::leadership(&role) {
            return Err(PlatformError::Forbidden);
        }
        if let Some(target) = target {
            LaborCalendarPolicy::developer(tx, target)?;
            if role != "super_admin" && !LaborCalendarPolicy::eligible(tx, ctx.actor_id(), target)?
            {
                return Err(PlatformError::Forbidden);
            }
        }
        Ok(())
    }
    pub fn leadership(role: &str) -> bool {
        matches!(role, "technical_lead" | "pmo" | "manager" | "super_admin")
    }
}
#[cfg(test)]
mod tests {
    use super::DailyDeveloperAnalysisPolicy;
    #[test]
    fn only_leadership_reads_analysis() {
        for role in ["technical_lead", "pmo", "manager", "super_admin"] {
            assert!(DailyDeveloperAnalysisPolicy::leadership(role));
        }
        for role in ["developer", "agent", "", "admin"] {
            assert!(!DailyDeveloperAnalysisPolicy::leadership(role));
        }
    }
}
