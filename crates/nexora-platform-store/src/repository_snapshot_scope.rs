use crate::{
    PlatformStore, local_snapshot_scope::LocalSnapshotScope, repository_access::RepositoryAccess,
    repository_binding_store::RepositoryBindingStore,
};
use nexora_core::{RepositoryId, SnapshotId, TenantId};
use nexora_platform::{AuthContext, LocalSnapshotOrigin, PlatformError, RepositorySourceKind};
use nexora_runtime::{DatabaseRuntime, QuerySnapshot};
use postgres::Transaction;

/// Server-selected published source; its authority locks live in the caller's transaction.
pub(crate) struct RepositorySnapshotScope {
    pub project_id: String,
    pub repository_id: String,
    pub runtime_tenant_id: String,
    pub runtime_repository_id: String,
    pub snapshot: String,
    pub expected_hash: Option<String>,
    pub commit: Option<String>,
    pub source_generation: i64,
    pub binding_revision: i64,
    pub local_origin: Option<LocalSnapshotOrigin>,
    pub local_authority: Option<serde_json::Value>,
}
impl RepositorySnapshotScope {
    pub fn connection(
        &self,
        tx: &mut Transaction<'_>,
    ) -> Result<(Option<String>, Option<i64>), PlatformError> {
        if self.local_origin.is_some() {
            return Ok((None, None));
        }
        let row = tx.query_one("SELECT connection_id,connection_revision FROM repository_bindings WHERE repository_id=$1 AND project_id=$2", &[&self.repository_id,&self.project_id]).map_err(PlatformStore::database_error)?;
        Ok((Some(row.get(0)), Some(row.get(1))))
    }
    pub fn lock(
        tx: &mut Transaction<'_>,
        ctx: &AuthContext,
        project: &str,
        repository: &str,
        snapshot: Option<&str>,
    ) -> Result<Self, PlatformError> {
        if snapshot.is_some_and(|id| id.is_empty() || id.len() > 4096) {
            return Err(PlatformError::Validation("invalid snapshot identifier"));
        }
        let access = RepositoryAccess::lock(tx, ctx, project, repository)?;
        if access.source_kind == RepositorySourceKind::LocalSnapshot {
            return LocalSnapshotScope::lock(tx, &access, project, repository, snapshot);
        }
        let binding = RepositoryBindingStore::lock_access(tx, ctx, project, repository, &access)?;
        let selected = snapshot.unwrap_or(&binding.snapshot_id).to_owned();
        let commit = if selected == binding.snapshot_id {
            binding.pinned_commit.clone()
        } else {
            // Staging does not grant access. Historical publication must share current consent.
            tx.query_opt("SELECT j.pinned_commit FROM repository_jobs j JOIN repository_bindings b ON b.repository_id=j.repository_id AND b.project_id=j.project_id AND b.source_generation=j.source_generation AND b.connection_id=j.connection_id AND b.connection_revision=j.connection_revision WHERE j.project_id=$1 AND j.repository_id=$2 AND j.snapshot_id=$3 AND j.state='succeeded' ORDER BY j.finished_at,j.id LIMIT 1", &[&project,&repository,&selected])
                .map_err(PlatformStore::database_error)?
                .ok_or(PlatformError::NotFound)?
                .get::<_, Option<String>>(0).ok_or(PlatformError::Storage)?
        };
        let expected_hash = (selected == binding.snapshot_id).then_some(binding.payload_hash);
        Ok(Self {
            project_id: project.into(),
            repository_id: repository.into(),
            runtime_tenant_id: binding.runtime_tenant_id,
            runtime_repository_id: binding.runtime_repository_id,
            snapshot: selected,
            expected_hash,
            commit: Some(commit),
            source_generation: binding.source_generation,
            binding_revision: binding.revision,
            local_origin: None,
            local_authority: None,
        })
    }
    pub fn authority(&self, tx: &mut Transaction<'_>) -> Result<serde_json::Value, PlatformError> {
        if let Some(authority) = &self.local_authority {
            return Ok(authority.clone());
        }
        let repository = &self.repository_id;
        let project = &self.project_id;
        let source = tx.query_one("SELECT connection_id,connection_revision FROM repository_bindings WHERE repository_id=$1 AND project_id=$2", &[&repository,&project]).map_err(PlatformStore::database_error)?;
        let authority = serde_json::json!({"tenant":self.runtime_tenant_id,"repository":self.runtime_repository_id,"source_generation":self.source_generation,"connection_id":source.get::<_,String>(0),"connection_revision":source.get::<_,i64>(1)});
        Ok(authority)
    }
    pub fn verify(&self, runtime: &mut DatabaseRuntime) -> Result<QuerySnapshot, PlatformError> {
        let header = runtime
            .query_header(
                &TenantId(self.runtime_tenant_id.clone()),
                &RepositoryId(self.runtime_repository_id.clone()),
                Some(&SnapshotId(self.snapshot.clone())),
            )
            .map_err(|_| PlatformError::Storage)?;
        if self
            .expected_hash
            .as_ref()
            .is_some_and(|expected| expected != &header.payload_hash)
        {
            return Err(PlatformError::Conflict(
                "published snapshot checksum changed",
            ));
        }
        Ok(header)
    }
}
