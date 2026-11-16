use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReuseStatistics {
    pub plan_reused: bool,
    pub cache_reset: bool,
    pub read_files: usize,
    pub source_bytes_read: usize,
    pub analyzed_files: usize,
    pub reused_files: usize,
    pub removed_files: usize,
    pub retained_files: usize,
    pub cache_skipped_files: usize,
    pub cache_weight_bytes: usize,
}
