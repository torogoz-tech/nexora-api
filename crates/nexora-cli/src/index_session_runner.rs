use crate::{index_session_pass::IndexSessionPass, index_session_report::IndexSessionReport};
use nexora_core::{NexoraError, TenantId};
use nexora_indexer::{IndexCache, Indexer, MemoryStore};
use std::path::Path;
pub struct IndexSessionRunner;
impl IndexSessionRunner {
    pub fn run(
        path: &Path,
        tenant: TenantId,
        indexer: Indexer,
        count: u16,
        capacity: usize,
    ) -> Result<IndexSessionReport, NexoraError> {
        if !(2..=100).contains(&count) {
            return Err(NexoraError::Configuration(
                "session requires 2..100 passes".into(),
            ));
        }
        let mut cache = IndexCache::with_capacity(capacity)?;
        let mut store = MemoryStore::default();
        let mut passes = Vec::new();
        let mut snapshot = None;
        for pass in 1..=count {
            let result = indexer.index_incremental(path, tenant.clone(), &mut store, &mut cache)?;
            passes.push(IndexSessionPass {
                pass,
                snapshot_id: result.snapshot.id.clone(),
                timings: result.timings,
                reuse: result.reuse,
            });
            snapshot = Some(result.snapshot);
        }
        Ok(IndexSessionReport {
            schema_version: 1,
            analysis_engine: nexora_indexer::ANALYSIS_ENGINE_VERSION.into(),
            passes,
            final_snapshot: snapshot
                .ok_or_else(|| NexoraError::Configuration("empty session".into()))?,
        })
    }
}
