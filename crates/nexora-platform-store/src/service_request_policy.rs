use crate::{PlatformStore, platform_authority::PlatformAuthority};
use nexora_platform::*;
use postgres::Transaction;
pub(crate) struct ServiceRequestPolicy;
impl ServiceRequestPolicy {
    pub fn role(tx: &mut Transaction<'_>, ctx: &AuthContext) -> Result<String, PlatformError> {
        PlatformAuthority::require(tx, ctx, Capability::RequestsRead, None)?;
        tx.query_opt(
            "SELECT role FROM actor_roles WHERE actor_id=$1 FOR SHARE",
            &[&ctx.actor_id()],
        )
        .map_err(PlatformStore::database_error)?
        .map(|r| r.get(0))
        .ok_or(PlatformError::Forbidden)
    }
    pub fn internal(role: &str) -> bool {
        matches!(
            role,
            "developer" | "technical_lead" | "pmo" | "manager" | "super_admin"
        )
    }
    pub fn visible(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        id: &str,
        lock: bool,
    ) -> Result<String, PlatformError> {
        let role = Self::role(tx, ctx)?;
        let query = format!(
            "SELECT id FROM service_requests WHERE id=$1 AND ($2 OR requester_id=$3){}",
            if lock { " FOR UPDATE" } else { "" }
        );
        tx.query_opt(&query, &[&id, &(role != "user"), &ctx.actor_id()])
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::NotFound)?;
        Ok(role)
    }
    pub fn criticality(input: &serde_json::Value) -> Result<&str, PlatformError> {
        let level = input
            .get("criticality")
            .map(|v| v.as_str().unwrap_or(""))
            .unwrap_or("medium");
        if !matches!(level, "low" | "medium" | "high" | "critical") {
            return Err(PlatformError::Validation("invalid request criticality"));
        }
        Ok(level)
    }
    pub fn validate_sla(value: &serde_json::Value) -> Result<(), PlatformError> {
        let Some(levels) = value.as_object() else {
            return Err(PlatformError::Validation("invalid SLA configuration"));
        };
        for (level, policy) in levels {
            if !matches!(level.as_str(), "low" | "medium" | "high" | "critical")
                || !matches!(policy["clock"].as_str(), Some("business" | "continuous"))
            {
                return Err(PlatformError::Validation("invalid SLA configuration"));
            }
            let response = policy["response_minutes"].as_i64().unwrap_or(0);
            let resolution = policy["resolution_minutes"].as_i64().unwrap_or(0);
            if !(1..=525600).contains(&response) || !(response..=525600).contains(&resolution) {
                return Err(PlatformError::Validation("invalid SLA minutes"));
            }
        }
        Ok(())
    }
    pub fn text<'a>(
        v: &'a serde_json::Value,
        name: &str,
        max: usize,
    ) -> Result<&'a str, PlatformError> {
        let s = v[name]
            .as_str()
            .ok_or(PlatformError::Validation("missing request field"))?
            .trim();
        if s.is_empty() || s.len() > max {
            return Err(PlatformError::Validation("invalid request field"));
        }
        Ok(s)
    }
}
