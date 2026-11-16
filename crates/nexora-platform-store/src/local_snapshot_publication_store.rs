use crate::PlatformStore;
use nexora_platform::{LocalSnapshotPublication, PlatformError};
use postgres::Transaction;
use serde_json::Value;

pub(crate) struct LocalSnapshotPublicationStore;
impl LocalSnapshotPublicationStore {
    pub fn get(
        tx: &mut Transaction<'_>,
        source: &str,
        publication: &str,
    ) -> Result<LocalSnapshotPublication, PlatformError> {
        let value: Value = tx.query_opt("SELECT jsonb_build_object('project_id',s.project_id,'repository_id',s.repository_id,'source_id',s.id,'publication_id',p.id,'runtime_tenant_id',s.runtime_tenant_id,'runtime_repository_id',s.runtime_repository_id,'snapshot_id',p.snapshot_id,'payload_hash',p.payload_hash,'source_revision',s.revision,'authorization_revision',s.authorization_revision,'source_generation',s.source_generation,'binding_revision',p.binding_revision,'attested_by',p.attested_by,'published_at',p.published_at) FROM local_repository_sources s JOIN local_snapshot_publications p ON p.source_id=s.id WHERE s.id=$1 AND p.id=$2", &[&source,&publication])
            .map_err(PlatformStore::database_error)?.ok_or(PlatformError::NotFound)?.get(0);
        PlatformStore::decode(value)
    }
    /// A receipt never restores retired trust or grants access on its own.
    pub fn lock_receipt(
        tx: &mut Transaction<'_>,
        value: &LocalSnapshotPublication,
    ) -> Result<(), PlatformError> {
        let owner: String = tx.query_opt("SELECT owner_actor_id FROM local_repository_sources WHERE id=$1 AND project_id=$2 AND repository_id=$3 AND state='active' AND authorization_revision=$4 AND source_generation=$5 FOR SHARE", &[&value.source_id,&value.project_id,&value.repository_id,&value.authorization_revision,&value.source_generation])
            .map_err(PlatformStore::database_error)?.ok_or(PlatformError::Conflict("local source authorization changed"))?.get(0);
        tx.query_opt(
            "SELECT id FROM actors WHERE id=$1 AND active FOR SHARE",
            &[&owner],
        )
        .map_err(PlatformStore::database_error)?
        .ok_or(PlatformError::Conflict("local source owner is inactive"))?;
        tx.query_opt("SELECT id FROM project_repositories WHERE id=$1 AND project_id=$2 AND source_kind='local_snapshot' AND NOT archived AND authorization_status='authorized' AND source_generation=$3 FOR SHARE", &[&value.repository_id,&value.project_id,&value.source_generation])
            .map_err(PlatformStore::database_error)?.ok_or(PlatformError::Conflict("local repository is unavailable"))?;
        tx.query_opt("SELECT id FROM local_snapshot_publications WHERE id=$1 AND source_id=$2 AND authorization_revision=$3 AND snapshot_id=$4 AND payload_hash=$5 FOR SHARE", &[&value.publication_id,&value.source_id,&value.authorization_revision,&value.snapshot_id,&value.payload_hash])
            .map_err(PlatformStore::database_error)?.ok_or(PlatformError::Conflict("local publication is unavailable"))?;
        Ok(())
    }
}
