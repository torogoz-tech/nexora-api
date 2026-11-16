use nexora_core::{NexoraError, RepositoryId, SnapshotId, TenantId};
use nexora_storage::{MemoryStore, Snapshot, SnapshotStore};
#[derive(Default)]
pub struct CountingStore {
    pub publications: usize,
    inner: MemoryStore,
}
impl SnapshotStore for CountingStore {
    fn publish(&mut self, snapshot: Snapshot) -> Result<(), NexoraError> {
        self.inner.publish(snapshot)?;
        self.publications += 1;
        Ok(())
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
