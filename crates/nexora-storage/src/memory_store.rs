use crate::{Snapshot, SnapshotStore};
use nexora_core::{NexoraError, RepositoryId, SnapshotId, TenantId};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct MemoryStore {
    snapshots: BTreeMap<(TenantId, RepositoryId, SnapshotId), Snapshot>,
}
impl SnapshotStore for MemoryStore {
    fn publish(&mut self, snapshot: Snapshot) -> Result<(), NexoraError> {
        nexora_graph::GraphIntegrityValidator::validate(&snapshot.graph)
            .map_err(|error| NexoraError::Storage(error.to_string()))?;
        let key = (
            snapshot.tenant_id.clone(),
            snapshot.repository_id.clone(),
            snapshot.id.clone(),
        );
        if self.snapshots.contains_key(&key) {
            return Err(NexoraError::Storage("snapshot is immutable".into()));
        }
        self.snapshots.insert(key, snapshot);
        Ok(())
    }
    fn get(
        &self,
        tenant: &TenantId,
        repository: &RepositoryId,
        snapshot: &SnapshotId,
    ) -> Option<&Snapshot> {
        self.snapshots
            .get(&(tenant.clone(), repository.clone(), snapshot.clone()))
    }
}
