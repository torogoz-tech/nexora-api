use nexora_core::{NexoraError, TenantId};
use nexora_indexer::{IncrementalSnapshot, IndexCache, Indexer, MeasuredSnapshot};
use nexora_storage::SnapshotStore;
use std::path::Path;
pub struct BenchmarkComparison;
impl BenchmarkComparison {
    pub fn measure<S: SnapshotStore, T: SnapshotStore>(
        indexer: &Indexer,
        path: &Path,
        tenant: TenantId,
        cache: &mut IndexCache,
        incremental_store: &mut S,
        full_store: &mut T,
    ) -> Result<(IncrementalSnapshot, MeasuredSnapshot), NexoraError> {
        let incremental =
            indexer.index_incremental(path, tenant.clone(), incremental_store, cache)?;
        let full = indexer.index_measured(path, tenant, full_store)?;
        Ok((incremental, full))
    }
}
