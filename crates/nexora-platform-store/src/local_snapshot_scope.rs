use crate::{
    PlatformStore, repository_access::RepositoryAccess,
    repository_snapshot_scope::RepositorySnapshotScope,
};
use nexora_platform::{LocalSnapshotOrigin, PlatformError};
use postgres::{Transaction, types::Type};
use serde_json::json;

pub(crate) struct LocalSnapshotScope;
impl LocalSnapshotScope {
    pub fn lock(
        tx: &mut Transaction<'_>,
        access: &RepositoryAccess,
        project: &str,
        repository: &str,
        snapshot: Option<&str>,
    ) -> Result<RepositorySnapshotScope, PlatformError> {
        let source = tx.query_typed_opt("SELECT id,owner_actor_id,runtime_tenant_id,runtime_repository_id,authorization_revision,source_generation,binding_revision,current_publication_id FROM local_repository_sources WHERE repository_id=$1 AND project_id=$2 AND state='active' FOR SHARE", &[(&repository, Type::TEXT),(&project, Type::TEXT)])
            .map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        let source_id: String = source.get(0);
        let owner: String = source.get(1);
        let tenant: String = source.get(2);
        let runtime_repository: String = source.get(3);
        let epoch: i64 = source.get(4);
        let generation: i64 = source.get(5);
        let binding_revision: i64 = source.get(6);
        let current_publication: String = source.get(7);
        tx.query_typed_opt(
            "SELECT id FROM actors WHERE id=$1 AND active FOR SHARE",
            &[(&owner, Type::TEXT)],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::NotFound)?;
        tx.query_typed_opt("SELECT id FROM project_repositories WHERE id=$1 AND project_id=$2 AND revision=$3 AND source_kind='local_snapshot' AND source_generation=$4 AND NOT archived AND authorization_status='authorized' FOR SHARE", &[(&repository, Type::TEXT),(&project, Type::TEXT),(&access.revision, Type::INT8),(&generation, Type::INT8)])
            .map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        let published = tx.query_typed_opt("SELECT id,snapshot_id,payload_hash FROM local_snapshot_publications WHERE source_id=$1 AND authorization_revision=$2 AND (($3::text IS NULL AND id=$4) OR ($3::text IS NOT NULL AND snapshot_id=$3)) ORDER BY binding_revision DESC,id LIMIT 1 FOR SHARE", &[(&source_id, Type::TEXT),(&epoch, Type::INT8),(&snapshot, Type::TEXT),(&current_publication, Type::TEXT)])
            .map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        let origin = LocalSnapshotOrigin {
            source_id: source_id.clone(),
            authorization_revision: epoch,
            publication_id: published.get(0),
        };
        let authority = json!({"tenant":tenant,"repository":runtime_repository,"source_kind":"local_snapshot",
            "source_id":source_id,"source_generation":generation,"authorization_revision":epoch,"owner_actor_id":owner});
        Ok(RepositorySnapshotScope {
            project_id: project.into(),
            repository_id: repository.into(),
            runtime_tenant_id: tenant,
            runtime_repository_id: runtime_repository,
            snapshot: published.get(1),
            expected_hash: Some(published.get(2)),
            commit: None,
            source_generation: generation,
            binding_revision,
            local_origin: Some(origin),
            local_authority: Some(authority),
        })
    }
}
