use crate::{
    PlatformStore, repository_access::RepositoryAccess, repository_job_source::RepositoryJobSource,
};
use nexora_platform::{AuthContext, PlatformError, RepositoryBinding, RepositorySourceKind};
use postgres::Transaction;

pub(crate) struct RepositoryBindingStore;
impl RepositoryBindingStore {
    pub fn get(
        store: &mut PlatformStore,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
    ) -> Result<RepositoryBinding, PlatformError> {
        let mut tx = store
            .client
            .transaction()
            .map_err(PlatformStore::database_error)?;
        let binding = Self::lock(&mut tx, ctx, project, repository)?;
        tx.commit().map_err(PlatformStore::database_error)?;
        Ok(binding)
    }
    pub(crate) fn lock(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
    ) -> Result<RepositoryBinding, PlatformError> {
        let access = RepositoryAccess::lock(tx, ctx, project, repository)?;
        Self::lock_access(tx, ctx, project, repository, &access)
    }
    pub(crate) fn lock_access(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        access: &RepositoryAccess,
    ) -> Result<RepositoryBinding, PlatformError> {
        if access.source_kind != RepositorySourceKind::Git {
            return Err(PlatformError::NotFound);
        }
        let source =
            RepositoryJobSource::lock_published(tx, ctx, project, repository, access.revision)
                .map_err(RepositoryAccess::conceal)?;
        // Lock the binding only after authority, project, connection and repository,
        // matching publication order. Never accept a client-selected runtime scope.
        // Metadata-only revisions do not change a stable source's code identity.
        let row = tx.query_opt("SELECT b.project_id,b.repository_id,b.runtime_tenant_id,b.runtime_repository_id,b.snapshot_id,b.payload_hash,b.pinned_commit,b.source_generation,b.job_id,b.attempt,b.revision,b.published_at::text FROM repository_bindings b JOIN installation i ON i.id=b.runtime_tenant_id AND i.singleton WHERE b.repository_id=$1 AND b.project_id=$2 AND b.runtime_repository_id=$1 AND b.source_generation=$3 AND b.connection_id=$4 AND b.connection_revision=$5 FOR SHARE OF b", &[&repository,&project,&source.generation,&source.connection,&source.connection_revision])
            .map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?;
        let binding = RepositoryBinding {
            project_id: row.get(0),
            repository_id: row.get(1),
            runtime_tenant_id: row.get(2),
            runtime_repository_id: row.get(3),
            snapshot_id: row.get(4),
            payload_hash: row.get(5),
            pinned_commit: row.get(6),
            source_generation: row.get(7),
            job_id: row.get(8),
            attempt: row.get(9),
            revision: row.get(10),
            published_at: row.get(11),
        };
        Ok(binding)
    }
}
