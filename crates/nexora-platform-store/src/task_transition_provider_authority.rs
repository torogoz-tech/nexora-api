use crate::{
    PlatformStore, development_policy_store::DevelopmentPolicyStore,
    platform_authority::PlatformAuthority, repository_job_source::RepositoryJobSource,
};
use nexora_platform::*;
use postgres::Transaction;
use serde_json::{Value, json};
pub(crate) struct TaskTransitionProviderAuthority;
impl TaskTransitionProviderAuthority {
    pub fn lock(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
    ) -> Result<Value, PlatformError> {
        PlatformAuthority::require(tx, ctx, Capability::RepositoriesRead, Some(project))?;
        PlatformAuthority::project(tx, project)?;
        let metadata:Value=tx.query_opt("SELECT repository_metadata(r) FROM project_repositories r WHERE id=$1 AND project_id=$2 AND NOT archived",&[&repository,&project]).map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        let repo: ProjectRepository = PlatformStore::decode(metadata)?;
        let source = RepositoryJobSource::lock(tx, ctx, project, repository, repo.revision)?;
        let revision: i64 = tx
            .query_one(
                "SELECT credential_revision FROM git_connections WHERE id=$1 FOR SHARE",
                &[&source.connection],
            )
            .map_err(PlatformStore::database_error)?
            .get(0);
        let policy = DevelopmentPolicyStore::lock(tx, project, repository)?;
        Ok(
            json!({"repository_id":repository,"repository_revision":repo.revision,"remote_id":repo.remote_id,"remote_full_name":repo.remote_full_name,"provider":repo.provider,"host":repo.host,"source_generation":source.generation,"connection_id":source.connection,"connection_revision":source.connection_revision,"credential_revision":revision,"policy_revision":policy.revision}),
        )
    }
}
