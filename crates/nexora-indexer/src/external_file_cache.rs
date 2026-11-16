use crate::{CacheFragment, FileCacheKey};
use nexora_core::NexoraError;
/// Standard callbacks keep database clients outside the analysis core.
pub struct ExternalFileCache<'a> {
    pub read: &'a mut dyn FnMut(&FileCacheKey) -> Result<Option<CacheFragment>, NexoraError>,
    pub write: &'a mut dyn FnMut(&FileCacheKey, &CacheFragment) -> Result<bool, NexoraError>,
}
