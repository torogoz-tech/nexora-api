use crate::{PlatformStore, audit_writer::AuditWriter, receipt_writer::ReceiptWriter};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::{Value, json};
pub(crate) struct LaborCalendarPolicy;
impl LaborCalendarPolicy {
    pub fn session(tx: &mut Transaction<'_>, ctx: &AuthContext) -> Result<(), PlatformError> {
        if ctx.method() != AuthMethod::Session || ctx.kind() != ActorKind::Human {
            return Err(PlatformError::Forbidden);
        }
        tx.query_opt("SELECT s.id FROM sessions s JOIN actors a ON a.id=s.actor_id JOIN users u ON u.actor_id=a.id WHERE s.id=$1 AND a.id=$2 AND a.active AND a.kind='human' AND s.revoked_at IS NULL AND s.expires_at>clock_timestamp() FOR SHARE OF s,a,u",&[&ctx.credential_id(),&ctx.actor_id()]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Unauthorized)?;
        // The caller's actor is locked before its authority rows, matching role writes.
        // Other members are locked individually, actor first, when their policy is checked.
        tx.query(
            "SELECT actor_id FROM actor_roles WHERE actor_id=$1 FOR SHARE",
            &[&ctx.actor_id()],
        )
        .map_err(PlatformStore::database_error)?;
        tx.query(
            "SELECT actor_id FROM member_role_projects WHERE actor_id=$1 FOR SHARE",
            &[&ctx.actor_id()],
        )
        .map_err(PlatformStore::database_error)?;
        tx.query(
            "SELECT actor_id FROM actor_grants WHERE actor_id=$1 FOR SHARE",
            &[&ctx.actor_id()],
        )
        .map_err(PlatformStore::database_error)?;
        Ok(())
    }
    pub fn admin(tx: &mut Transaction<'_>, actor: &str) -> Result<bool, PlatformError> {
        Ok(tx.query_one("SELECT EXISTS(SELECT 1 FROM actor_grants WHERE actor_id=$1 AND capability='installation:manage' AND project_id IS NULL)",&[&actor]).map_err(PlatformStore::database_error)?.get(0))
    }
    pub fn member(tx: &mut Transaction<'_>, actor: &str) -> Result<(), PlatformError> {
        tx.query_opt("SELECT a.id FROM actors a JOIN users u ON u.actor_id=a.id WHERE a.id=$1 AND a.active AND a.kind='human' FOR SHARE OF a,u", &[&actor])
            .map_err(PlatformStore::database_error)?.ok_or(PlatformError::Forbidden)?;
        Ok(())
    }
    pub fn activity_editor(tx: &mut Transaction<'_>, actor: &str) -> Result<bool, PlatformError> {
        Ok(Self::admin(tx, actor)? || tx.query_one("SELECT EXISTS(SELECT 1 FROM actor_roles WHERE actor_id=$1 AND role IN ('pmo','manager','technical_lead'))", &[&actor])
            .map_err(PlatformStore::database_error)?.get::<_,bool>(0))
    }
    pub fn developer(tx: &mut Transaction<'_>, actor: &str) -> Result<(), PlatformError> {
        tx.query_opt(
            "SELECT id FROM actors WHERE id=$1 AND active FOR SHARE",
            &[&actor],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::Forbidden)?;
        if !tx.query_one("SELECT EXISTS(SELECT 1 FROM actor_roles r JOIN actors a ON a.id=r.actor_id WHERE a.id=$1 AND a.active AND a.kind='human' AND r.role IN ('developer','technical_lead'))",&[&actor]).map_err(PlatformStore::database_error)?.get::<_,bool>(0){return Err(PlatformError::Forbidden)}
        Ok(())
    }
    pub fn eligible(
        tx: &mut Transaction<'_>,
        approver: &str,
        target: &str,
    ) -> Result<bool, PlatformError> {
        if tx
            .query_opt(
                "SELECT id FROM actors WHERE id=$1 AND active FOR SHARE",
                &[&approver],
            )
            .map_err(PlatformStore::database_error)?
            .is_none()
        {
            return Ok(false);
        }
        Ok(tx.query_one("SELECT EXISTS(SELECT 1 FROM actor_roles r JOIN actors a ON a.id=r.actor_id WHERE r.actor_id=$1 AND a.active AND a.kind='human' AND r.role IN ('pmo','manager','technical_lead') AND (r.all_projects OR (EXISTS(SELECT 1 FROM member_role_projects WHERE actor_id=$2) AND NOT EXISTS(SELECT 1 FROM actor_roles WHERE actor_id=$2 AND all_projects) AND NOT EXISTS(SELECT 1 FROM member_role_projects p WHERE p.actor_id=$2 AND NOT EXISTS(SELECT 1 FROM member_role_projects q WHERE q.actor_id=$1 AND q.project_id=p.project_id)))))",&[&approver,&target]).map_err(PlatformStore::database_error)?.get(0))
    }
    pub fn may_decide(
        tx: &mut Transaction<'_>,
        actor: &str,
        target: &str,
    ) -> Result<bool, PlatformError> {
        if actor == target {
            return Ok(false);
        }
        if Self::admin(tx, actor)? {
            return Ok(true);
        }
        // The target actor is the shared serialization point for eligibility
        // and role writes. Decision/assignment mutations additionally lock the
        // calendar settings singleton, so even an absent assignment cannot race
        // an insert or clear between this check and the recorded decision.
        if tx
            .query_opt(
                "SELECT id FROM actors WHERE id=$1 AND active AND kind='human' FOR SHARE",
                &[&target],
            )
            .map_err(PlatformStore::database_error)?
            .is_none()
        {
            return Ok(false);
        }
        let assigned = tx
            .query_opt(
                "SELECT approver_id FROM labor_approvers WHERE actor_id=$1 FOR SHARE",
                &[&target],
            )
            .map_err(PlatformStore::database_error)?
            .and_then(|row| row.get::<_, Option<String>>(0));
        if let Some(assigned) = assigned {
            return Ok(assigned == actor && Self::eligible(tx, actor, target)?);
        }
        // Deliberately organization-wide only for unassigned developers. PMO
        // retains its assigned approver behavior; this fallback is manager/LT.
        Ok(tx.query_one("SELECT EXISTS(SELECT 1 FROM actor_roles r JOIN actors a ON a.id=r.actor_id WHERE r.actor_id=$1 AND a.active AND a.kind='human' AND r.role IN ('manager','technical_lead'))", &[&actor])
            .map_err(PlatformStore::database_error)?.get(0))
    }

    pub fn global_approver(tx: &mut Transaction<'_>, actor: &str) -> Result<bool, PlatformError> {
        Ok(Self::admin(tx,actor)? || tx.query_one("SELECT EXISTS(SELECT 1 FROM actor_roles WHERE actor_id=$1 AND role IN ('pmo','manager') AND all_projects)",&[&actor]).map_err(PlatformStore::database_error)?.get::<_,bool>(0))
    }
    pub fn text<'a>(v: &'a Value, key: &str, max: usize) -> Result<&'a str, PlatformError> {
        v[key]
            .as_str()
            .filter(|v| {
                !v.trim().is_empty()
                    && v.len() <= max
                    && !v.chars().any(|c| c.is_control() && c != '\n')
            })
            .ok_or(PlatformError::Validation("invalid labor calendar field"))
    }
    pub fn country(v: &Value) -> Result<Option<String>, PlatformError> {
        if v.is_null() {
            return Ok(None);
        }
        let s = v
            .as_str()
            .ok_or(PlatformError::Validation("invalid country"))?
            .trim()
            .to_ascii_uppercase();
        if s.len() != 2 || !s.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(PlatformError::Validation("invalid country"));
        }
        Ok(Some(s))
    }
    pub fn date(v: &Value, key: &str) -> Result<String, PlatformError> {
        let s = Self::text(v, key, 10)?;
        if s.len() != 10 {
            return Err(PlatformError::Validation("invalid date"));
        }
        let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]")
            .map_err(|_| PlatformError::Validation("invalid date"))?;
        time::Date::parse(s, &format).map_err(|_| PlatformError::Validation("invalid date"))?;
        Ok(s.to_owned())
    }
    pub fn finish(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        op: &str,
        target: Option<&str>,
        request: Option<&crate::mutation_request::MutationRequest>,
        value: &Value,
    ) -> Result<(), PlatformError> {
        AuditWriter::append(
            tx,
            Some(ctx.actor_id()),
            Some(ctx.credential_id()),
            op,
            target,
            json!({"result":value}),
        )?;
        ReceiptWriter::save(tx, request, value)
    }
}
