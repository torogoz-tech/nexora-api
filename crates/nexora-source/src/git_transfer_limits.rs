use crate::SourceError;
use std::time::Duration;
#[derive(Debug, Clone)]
pub struct GitTransferLimits {
    pub timeout: Duration,
    pub max_pack_bytes: u64,
    pub max_repository_bytes: u64,
    pub max_repository_entries: usize,
}
impl Default for GitTransferLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(60),
            max_pack_bytes: 256 * 1024 * 1024,
            max_repository_bytes: 2 * 1024 * 1024 * 1024,
            max_repository_entries: 250_000,
        }
    }
}
impl GitTransferLimits {
    pub fn validate(&self) -> Result<(), SourceError> {
        if self.timeout.is_zero()
            || self.timeout > Duration::from_secs(120)
            || self.max_pack_bytes == 0
            || self.max_repository_bytes < self.max_pack_bytes
            || self.max_repository_entries == 0
        {
            return Err(SourceError::InvalidConfiguration);
        }
        Ok(())
    }
}
