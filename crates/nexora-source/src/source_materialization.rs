use crate::GitTreeEntry;
use std::path::{Path, PathBuf};
/// Server-side capture metadata; deliberately not serializable and contains no executable handles.
pub struct SourceMaterialization {
    pub(crate) root: PathBuf,
    pub(crate) commit: String,
    pub(crate) files: Vec<GitTreeEntry>,
    pub(crate) total_bytes: u64,
}
impl SourceMaterialization {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn commit(&self) -> &str {
        &self.commit
    }
    pub fn files(&self) -> &[GitTreeEntry] {
        &self.files
    }
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
}
