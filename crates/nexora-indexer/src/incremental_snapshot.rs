pub struct IncrementalSnapshot {
    pub snapshot: nexora_storage::Snapshot,
    pub timings: crate::StageTimings,
    pub reuse: crate::ReuseStatistics,
}
