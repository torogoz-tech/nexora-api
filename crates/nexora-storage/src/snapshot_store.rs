use crate::Snapshot;
use nexora_core::{NexoraError, RepositoryId, SnapshotId, TenantId};
/// Atomic whole-snapshot publication. Implementations must isolate all three identifiers.
pub trait SnapshotStore {
    fn publish(&mut self, snapshot: Snapshot) -> Result<(), NexoraError>;
    fn get(
        &self,
        tenant: &TenantId,
        repository: &RepositoryId,
        snapshot: &SnapshotId,
    ) -> Option<&Snapshot>;
}
