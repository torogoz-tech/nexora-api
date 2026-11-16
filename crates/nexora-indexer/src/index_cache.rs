use crate::{cached_file::CachedFile, index_scope::IndexScope};
use std::{collections::BTreeMap, sync::Arc};
/// One committed generation of pre-resolution fragments. Weight is encoded data, not RSS.
pub struct IndexCache {
    pub(crate) scope: Option<IndexScope>,
    pub(crate) entries: BTreeMap<String, Arc<CachedFile>>,
    pub(crate) max_weight: usize,
    pub(crate) weight: usize,
}
impl IndexCache {
    pub fn with_capacity(max_weight: usize) -> Result<Self, nexora_core::NexoraError> {
        if max_weight > 67_108_864 {
            return Err(nexora_core::NexoraError::Configuration(
                "cache capacity > 64 MiB".into(),
            ));
        }
        Ok(Self {
            scope: None,
            entries: BTreeMap::new(),
            max_weight,
            weight: 0,
        })
    }
    pub fn retained_files(&self) -> usize {
        self.entries.len()
    }
    pub fn weight_bytes(&self) -> usize {
        self.weight
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.scope = None;
        self.weight = 0;
    }
}
impl Default for IndexCache {
    fn default() -> Self {
        Self {
            scope: None,
            entries: BTreeMap::new(),
            max_weight: 67_108_864,
            weight: 0,
        }
    }
}
