use crate::{
    PlatformStore, labor_calendar_policy::LaborCalendarPolicy,
    platform_authority::PlatformAuthority,
};
use nexora_platform::*;
use postgres::Transaction;
pub(crate) struct GitAuthorPolicy;
impl GitAuthorPolicy {
    pub fn lock(tx: &mut Transaction<'_>) -> Result<(), PlatformError> {
        tx.query_one("SELECT pg_advisory_xact_lock(hashtextextended(current_schema()||':git-author-aliases',0))",&[]).map_err(PlatformStore::database_error)?;
        Ok(())
    }
    pub fn projects(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: Option<&str>,
    ) -> Result<Vec<String>, PlatformError> {
        // Revalidate the live credential even when no projects are visible.
        match PlatformAuthority::require(tx, ctx, Capability::RepositoriesRead, project) {
            Ok(()) => {}
            Err(PlatformError::Forbidden) if project.is_none() => {}
            Err(e) => return Err(e),
        };
        let mut visible = Vec::new();
        let rows=tx.query("SELECT id FROM projects WHERE NOT archived AND ($1::text IS NULL OR id=$1) ORDER BY id",&[&project]).map_err(PlatformStore::database_error)?;
        for row in rows {
            let id: String = row.get(0);
            match PlatformAuthority::require(tx, ctx, Capability::RepositoriesRead, Some(&id)) {
                Ok(()) => visible.push(id),
                Err(PlatformError::Forbidden) => {}
                Err(e) => return Err(e),
            }
        }
        if project.is_some() && visible.is_empty() {
            return Err(PlatformError::NotFound);
        }
        Ok(visible)
    }
    pub fn manager(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: Option<&str>,
    ) -> Result<(), PlatformError> {
        LaborCalendarPolicy::session(tx, ctx)?;
        tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('super_admin','pmo','manager','technical_lead') AND ($2::text IS NULL OR all_projects OR ($2::text IS NOT NULL AND EXISTS(SELECT 1 FROM member_role_projects p WHERE p.actor_id=$1 AND p.project_id=$2))) FOR SHARE",&[&ctx.actor_id(),&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Forbidden)?;
        if project.is_none() {
            if Self::projects(tx, ctx, None)?.is_empty() {
                PlatformAuthority::require(tx, ctx, Capability::RepositoriesRead, None)?;
            }
        } else {
            PlatformAuthority::require(tx, ctx, Capability::RepositoriesRead, project)?;
        }
        if let Some(p) = project {
            PlatformAuthority::project(tx, p)?;
        }
        Ok(())
    }
    pub fn target(
        tx: &mut Transaction<'_>,
        actor: &str,
        project: Option<&str>,
    ) -> Result<(), PlatformError> {
        tx.query_opt("SELECT a.id FROM actors a WHERE a.id=$1 AND a.active AND a.kind='human' AND ($2::text IS NULL OR EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=a.id AND g.capability='repositories:read' AND (g.project_id IS NULL OR g.project_id=$2))) FOR SHARE OF a",&[&actor,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::Validation("author member is unavailable or outside project"))?;
        Ok(())
    }
}
