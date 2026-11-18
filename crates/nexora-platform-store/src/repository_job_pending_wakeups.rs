use crate::PlatformStore;
use nexora_platform::{PlatformError, RepositoryJob};

pub(crate) struct RepositoryJobPendingWakeups;

impl RepositoryJobPendingWakeups {
    pub(crate) fn list(store: &mut PlatformStore) -> Result<Vec<RepositoryJob>, PlatformError> {
        store
            .client
            .query(
                "SELECT DISTINCT ON (j.kind) to_jsonb(j) FROM repository_jobs j WHERE (j.state='queued' AND j.available_at<=clock_timestamp()) OR (j.state='running' AND j.lease_until<=clock_timestamp()) ORDER BY j.kind,j.created_at,j.id",
                &[],
            )
            .map_err(PlatformStore::database_error)?
            .into_iter()
            .map(|row| PlatformStore::decode(row.get(0)))
            .collect()
    }
}
