use crate::{PlatformStore, platform_authority::PlatformAuthority};
use nexora_platform::{AuthContext, Capability, PlatformError};
use postgres::Transaction;
pub(crate) struct KnowledgeContributionPolicy;
impl KnowledgeContributionPolicy {
    pub fn read(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: Option<&str>,
    ) -> Result<(), PlatformError> {
        if let Some(project) = project {
            PlatformAuthority::require(tx, ctx, Capability::DocumentsRead, Some(project))?;
            PlatformAuthority::project(tx, project)
        } else {
            PlatformAuthority::require(tx, ctx, Capability::OrganizationKnowledgeRead, None)
        }
    }
    pub fn write(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: Option<&str>,
        editing: bool,
    ) -> Result<(), PlatformError> {
        Self::read(tx, ctx, project)?;
        PlatformAuthority::require(
            tx,
            ctx,
            if editing {
                if project.is_some() {
                    Capability::DocumentsWrite
                } else {
                    Capability::OrganizationKnowledgeWrite
                }
            } else {
                Capability::KnowledgeContribute
            },
            project,
        )?;
        let roles = if editing {
            vec!["technical_lead", "pmo", "manager", "super_admin"]
        } else {
            vec![
                "developer",
                "technical_lead",
                "pmo",
                "manager",
                "super_admin",
            ]
        };
        tx.query_opt(
            "SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role=ANY($2) FOR SHARE",
            &[&ctx.actor_id(), &roles],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::Forbidden)?;
        Ok(())
    }
}
