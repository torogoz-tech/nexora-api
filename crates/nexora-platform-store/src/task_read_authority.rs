use crate::{
    PlatformStore, platform_authority::PlatformAuthority, task_read_predicate::task_read_predicate,
    task_read_scope::TaskReadScope,
};
use nexora_platform::{AuthContext, Capability, PlatformError};
use postgres::Transaction;
pub(crate) struct TaskReadAuthority;
impl TaskReadAuthority {
    pub fn unrestricted(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
    ) -> Result<bool, PlatformError> {
        for (capability, scope) in [
            (Capability::InstallationManage, None),
            (Capability::ActorsManage, None),
            (Capability::ProjectsWrite, Some(project)),
        ] {
            match PlatformAuthority::require(tx, ctx, capability, scope) {
                Ok(()) => return Ok(true),
                Err(PlatformError::Forbidden) => {}
                Err(error) => return Err(error),
            }
        }
        // Coordination roles can follow all work only inside their current project scope.
        // Keep API-key attenuation: the live TasksRead scope must also allow this project.
        match PlatformAuthority::require(tx, ctx, Capability::TasksRead, Some(project)) {
            Ok(()) => {}
            Err(PlatformError::Forbidden) => return Ok(false),
            Err(error) => return Err(error),
        }
        let role = tx.query_opt("SELECT all_projects FROM actor_roles WHERE actor_id=$1 AND role IN ('technical_lead','pmo','manager') FOR SHARE", &[&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        if let Some(role) = role {
            if role.get::<_, bool>(0) {
                return Ok(true);
            }
            return Ok(tx.query_opt("SELECT project_id FROM member_role_projects WHERE actor_id=$1 AND project_id=$2 FOR SHARE", &[&ctx.actor_id(), &project]).map_err(PlatformStore::database_error)?.is_some());
        }
        Ok(false)
    }
    pub fn scope(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
    ) -> Result<TaskReadScope, PlatformError> {
        // Validate live identity even when no project grant remains.
        match PlatformAuthority::require(tx, ctx, Capability::TasksRead, None) {
            Ok(()) | Err(PlatformError::Forbidden) => {}
            Err(error) => return Err(error),
        }
        let candidates=tx.query("SELECT id FROM projects p WHERE NOT archived AND EXISTS(SELECT 1 FROM actor_grants g WHERE g.actor_id=$1 AND g.capability='tasks:read' AND (g.project_id IS NULL OR g.project_id=p.id)) ORDER BY id",&[&ctx.actor_id()]).map_err(PlatformStore::database_error)?;
        let mut scope = TaskReadScope {
            projects: Vec::new(),
            unrestricted: Vec::new(),
        };
        for row in candidates {
            let project: String = row.get(0);
            match PlatformAuthority::require(tx, ctx, Capability::TasksRead, Some(&project)) {
                Ok(()) => {
                    if Self::unrestricted(tx, ctx, &project)? {
                        scope.unrestricted.push(project.clone());
                    }
                    scope.projects.push(project)
                }
                Err(PlatformError::Forbidden) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(scope)
    }
    pub fn require_task(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<(), PlatformError> {
        PlatformAuthority::require(tx, ctx, Capability::TasksRead, Some(project)).map_err(
            |error| {
                if matches!(error, PlatformError::Forbidden) {
                    PlatformError::NotFound
                } else {
                    error
                }
            },
        )?;
        Self::require_relation(tx, ctx, project, id)
    }
    pub fn require_relation(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        id: &str,
    ) -> Result<(), PlatformError> {
        let unrestricted = Self::unrestricted(tx, ctx, project)?;
        let predicate = task_read_predicate("$3", "$4");
        tx.query_opt(
            &format!("SELECT t.id FROM tasks t WHERE t.project_id=$1 AND t.id=$2 AND {predicate}"),
            &[&project, &id, &ctx.actor_id(), &unrestricted],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::NotFound)?;
        Ok(())
    }
}
