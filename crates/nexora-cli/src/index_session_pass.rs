use serde::Serialize;
#[derive(Serialize)]
pub struct IndexSessionPass {
    pub pass: u16,
    pub snapshot_id: nexora_core::SnapshotId,
    pub timings: nexora_indexer::StageTimings,
    pub reuse: nexora_indexer::ReuseStatistics,
}
