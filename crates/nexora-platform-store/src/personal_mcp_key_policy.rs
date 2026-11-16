use crate::{PlatformStore, platform_authority::PlatformAuthority};
use nexora_platform::{
    ActorKind, AuthContext, AuthMethod, Capability, KeyScope, PersonalMcpKeyCreate, PlatformError,
};
use postgres::Transaction;

pub(crate) struct PersonalMcpKeyPolicy;
impl PersonalMcpKeyPolicy {
    pub fn session(tx: &mut Transaction<'_>, ctx: &AuthContext) -> Result<(), PlatformError> {
        if ctx.method() != AuthMethod::Session || ctx.kind() != ActorKind::Human {
            return Err(PlatformError::Forbidden);
        }
        // An authenticated user needs no administrative grant to manage their own
        // delegated access. require still locks/revalidates the live actor/session.
        match PlatformAuthority::require(tx, ctx, Capability::KeysManage, None) {
            Ok(()) | Err(PlatformError::Forbidden) => {}
            Err(error) => return Err(error),
        }
        tx.query(
            "SELECT capability,project_id FROM actor_grants WHERE actor_id=$1 FOR SHARE",
            &[&ctx.actor_id()],
        )
        .map_err(PlatformStore::database_error)?;
        Ok(())
    }
    pub fn safe(capability: Capability, read_only: bool) -> bool {
        match capability {
            Capability::RequestsRead
            | Capability::MembersRead
            | Capability::ConnectionsRead
            | Capability::TaskTypesRead
            | Capability::AnalysisProfilesRead
            | Capability::OrganizationKnowledgeRead
            | Capability::ProjectsRead
            | Capability::RepositoriesRead
            | Capability::KnowledgeRead
            | Capability::WorkflowsRead
            | Capability::TasksRead
            | Capability::DocumentsRead => true,
            Capability::RequestsWrite
            | Capability::KnowledgeContribute
            | Capability::AnalysisProfilesContribute
            | Capability::OrganizationKnowledgeWrite
            | Capability::ProjectsWrite
            | Capability::RepositoriesWrite
            | Capability::RepositoriesSync
            | Capability::KnowledgeWrite
            | Capability::TasksWrite
            | Capability::TasksTransition
            | Capability::BacklogsWrite
            | Capability::DocumentsWrite
            | Capability::DocumentsPublish => !read_only,
            _ => false,
        }
    }
    pub fn scope(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        input: &PersonalMcpKeyCreate,
    ) -> Result<KeyScope, PlatformError> {
        if input.project_ids.len() > 256
            || input
                .project_ids
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != input.project_ids.len()
            || input
                .project_ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 128 || id.chars().any(char::is_control))
        {
            return Err(PlatformError::Validation("invalid MCP project identifiers"));
        }
        for project in &input.project_ids {
            PlatformAuthority::project(tx, project)?;
        }
        let grants = tx
            .query(
                "SELECT capability,project_id FROM actor_grants WHERE actor_id=$1 FOR SHARE",
                &[&ctx.actor_id()],
            )
            .map_err(PlatformStore::database_error)?;
        let allows = |capability: Capability, project: Option<&str>| {
            grants.iter().any(|row| {
                row.get::<_, String>(0) == capability.as_str()
                    && row
                        .get::<_, Option<String>>(1)
                        .as_deref()
                        .is_none_or(|p| Some(p) == project)
            })
        };
        // A personal key delegates the safe MCP surface, not a snapshot of grants.
        // AuthContext and PlatformAuthority intersect this envelope with live grants
        // on every request. Human-only approvals/administration stay excluded.
        let external = tx.query_opt("SELECT actor_id FROM actor_roles WHERE actor_id=$1 AND role IN ('user','supervisor')", &[&ctx.actor_id()]).map_err(PlatformStore::database_error)?.is_some();
        let capabilities = Capability::all()
            .into_iter()
            .filter(|cap| {
                Self::safe(*cap, input.read_only)
                    && (!external
                        || matches!(cap, Capability::RequestsRead | Capability::RequestsWrite))
            })
            .collect::<Vec<_>>();
        if !capabilities.iter().any(|cap| {
            grants
                .iter()
                .any(|row| row.get::<_, String>(0) == cap.as_str())
        }) {
            return Err(PlatformError::Forbidden);
        }
        // Older clients may still send project selections. Validate them, but do
        // not persist a second project-access list that can drift from membership.
        for project in &input.project_ids {
            if !allows(Capability::ProjectsRead, Some(project)) {
                return Err(PlatformError::Forbidden);
            }
        }
        let scope = KeyScope {
            capabilities,
            project_ids: Vec::new(),
            all_projects: !external,
            installation: true,
        };
        scope.validate()?;
        Ok(scope)
    }
}
