use std::path::{Path, PathBuf};
pub struct FetchedGitSource {
    pub(crate) bare: PathBuf,
    pub(crate) commit: String,
    pub(crate) repository_bytes: u64,
}
impl FetchedGitSource {
    pub fn bare(&self) -> &Path {
        &self.bare
    }
    pub fn commit(&self) -> &str {
        &self.commit
    }
    pub fn repository_bytes(&self) -> u64 {
        self.repository_bytes
    }
}
