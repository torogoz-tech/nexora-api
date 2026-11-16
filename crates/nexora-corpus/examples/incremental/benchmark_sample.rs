use serde::Serialize;
#[derive(Serialize)]
pub struct BenchmarkSample {
    pub iteration: usize,
    pub phase: String,
    pub timings: nexora_indexer::StageTimings,
    pub reuse: nexora_indexer::ReuseStatistics,
    pub full_scan_timings: nexora_indexer::StageTimings,
    pub equivalent: bool,
}
