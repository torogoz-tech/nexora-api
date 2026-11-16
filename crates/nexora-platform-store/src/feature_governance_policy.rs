use crate::{PlatformStore, platform_authority::PlatformAuthority};
use nexora_platform::*;
use postgres::Transaction;
pub(crate) struct FeatureGovernancePolicy;
impl FeatureGovernancePolicy {
    pub fn read(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
    ) -> Result<(), PlatformError> {
        PlatformAuthority::require(tx, ctx, Capability::DocumentsRead, Some(project))?;
        PlatformAuthority::require(tx, ctx, Capability::TasksRead, Some(project))?;
        PlatformAuthority::project(tx, project)
    }
    pub fn human(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
    ) -> Result<(), PlatformError> {
        if ctx.method() != AuthMethod::Session || ctx.kind() != ActorKind::Human {
            return Err(PlatformError::Forbidden);
        }
        PlatformAuthority::require(tx, ctx, Capability::ApprovalsGrant, Some(project))?;
        let role = tx
            .query_opt(
                "SELECT role,all_projects FROM actor_roles WHERE actor_id=$1 FOR SHARE",
                &[&ctx.actor_id()],
            )
            .map_err(PlatformStore::database_error)?
            .ok_or(PlatformError::Forbidden)?;
        let name: String = role.get(0);
        let all: bool = role.get(1);
        if !matches!(
            name.as_str(),
            "technical_lead" | "pmo" | "manager" | "super_admin"
        ) {
            return Err(PlatformError::Forbidden);
        }
        if !all&&tx.query_opt("SELECT project_id FROM member_role_projects WHERE actor_id=$1 AND project_id=$2 FOR SHARE",&[&ctx.actor_id(),&project]).map_err(PlatformStore::database_error)?.is_none(){return Err(PlatformError::Forbidden)}
        PlatformAuthority::project(tx, project)
    }
    pub fn catalog(tx: &mut Transaction<'_>, project: &str) -> Result<i64, PlatformError> {
        tx.execute(
            "INSERT INTO feature_catalogs(project_id) VALUES($1) ON CONFLICT DO NOTHING",
            &[&project],
        )
        .map_err(PlatformStore::database_error)?;
        Ok(tx
            .query_one(
                "SELECT revision FROM feature_catalogs WHERE project_id=$1 FOR UPDATE",
                &[&project],
            )
            .map_err(PlatformStore::database_error)?
            .get(0))
    }
    pub fn requirement(
        tx: &mut Transaction<'_>,
        project: &str,
        id: &str,
        version: &str,
    ) -> Result<String, PlatformError> {
        let row=tx.query_opt("SELECT v.title,d.latest_version_id FROM project_documents d JOIN project_document_versions v ON v.document_id=d.id AND v.project_id=d.project_id AND v.id=$3 WHERE d.id=$1 AND d.project_id=$2 AND d.kind='requirement' AND NOT d.archived FOR SHARE OF d,v",&[&id,&project,&version]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        if row.get::<_, String>(1) != version {
            return Err(PlatformError::Conflict(
                "requirement version changed; review the map again",
            ));
        }
        Ok(row.get(0))
    }
    pub fn links(
        tx: &mut Transaction<'_>,
        project: &str,
        links: &[FeatureLinkProposal],
    ) -> Result<(), PlatformError> {
        if links.is_empty() || links.len() > 32 {
            return Err(PlatformError::Validation("a map needs 1..32 features"));
        }
        let mut identities = std::collections::BTreeSet::new();
        for link in links {
            DomainText::required(&link.impact, 4000)?;
            if let Some(id) = &link.feature_id {
                if link.name.is_some() || link.description.is_some() {
                    return Err(PlatformError::Validation(
                        "existing features are linked, not recreated",
                    ));
                }
                let row=tx.query_opt("SELECT revision FROM project_features WHERE id=$1 AND project_id=$2 AND NOT archived FOR SHARE",&[&id,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
                if Some(row.get::<_, i64>(0)) != link.expected_revision {
                    return Err(PlatformError::Conflict("feature revision changed"));
                }
                if !identities.insert(format!("id:{id}")) {
                    return Err(PlatformError::Validation("duplicate feature in map"));
                }
            } else {
                let name = DomainText::name(link.name.as_deref().unwrap_or(""))?;
                if link.expected_revision.is_some() {
                    return Err(PlatformError::Validation("new features have no revision"));
                }
                DomainText::optional(link.description.as_deref().unwrap_or(""), 4000)?;
                if !identities.insert(format!("name:{}", name.to_lowercase())) {
                    return Err(PlatformError::Validation("duplicate feature in map"));
                }
                if tx.query_opt("SELECT id FROM project_features WHERE project_id=$1 AND lower(btrim(name))=lower(btrim($2))",&[&project,&name]).map_err(PlatformStore::database_error)?.is_some(){return Err(PlatformError::Conflict("feature already exists; link it instead of creating it"))}
            }
        }
        Ok(())
    }
}
