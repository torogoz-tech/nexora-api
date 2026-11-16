pub struct MeasuredSnapshot {
    pub snapshot: nexora_storage::Snapshot,
    pub timings: crate::StageTimings,
}
