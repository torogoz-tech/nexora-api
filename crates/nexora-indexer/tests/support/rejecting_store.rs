use nexora_core::{NexoraError, RepositoryId, SnapshotId, TenantId};
use nexora_storage::{MemoryStore, Snapshot, SnapshotStore};
#[derive(Default)]
pub struct RejectingStore {
    pub reject: bool,
    inner: MemoryStore,
}
impl SnapshotStore for RejectingStore {
    fn publish(&mut self, snapshot: Snapshot) -> Result<(), NexoraError> {
        if self.reject {
            Err(NexoraError::Storage("publication rejected".into()))
        } else {
            self.inner.publish(snapshot)
        }
    }
    fn get(
        &self,
        tenant: &TenantId,
        repository: &RepositoryId,
        snapshot: &SnapshotId,
    ) -> Option<&Snapshot> {
        self.inner.get(tenant, repository, snapshot)
    }
}
