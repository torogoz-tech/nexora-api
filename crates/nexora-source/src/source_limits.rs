use crate::SourceError;
#[derive(Debug, Clone)]
pub struct SourceLimits {
    pub max_tree_bytes: usize,
    pub max_entries: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_path_nodes: usize,
    pub max_path_bytes: usize,
    pub max_path_depth: usize,
}
impl Default for SourceLimits {
    fn default() -> Self {
        Self {
            max_tree_bytes: 128 * 1024 * 1024,
            max_entries: 250_000,
            max_file_bytes: 512 * 1024 * 1024,
            max_total_bytes: 2 * 1024 * 1024 * 1024,
            max_path_nodes: 250_000,
            max_path_bytes: 4096,
            max_path_depth: 256,
        }
    }
}
impl SourceLimits {
    pub fn validate(&self) -> Result<(), SourceError> {
        if self.max_tree_bytes == 0
            || self.max_entries == 0
            || self.max_file_bytes == 0
            || self.max_total_bytes == 0
            || self.max_path_nodes == 0
            || self.max_path_bytes == 0
            || self.max_path_depth == 0
        {
            return Err(SourceError::LimitExceeded);
        }
        Ok(())
    }
}
