use nexora_core::NexoraError;
use std::path::{Path, PathBuf};
pub struct RepositoryWalker;
impl RepositoryWalker {
    pub fn collect(root: &Path) -> Result<Vec<PathBuf>, NexoraError> {
        Ok(Self::collect_with_exclusions(root)?.0)
    }
    /// Excluded directory prefixes are recorded without traversing their contents.
    pub(crate) fn collect_with_exclusions(
        root: &Path,
    ) -> Result<(Vec<PathBuf>, Vec<String>), NexoraError> {
        let mut files = Vec::new();
        let mut excluded = Vec::new();
        let mut pending = vec![(root.to_path_buf(), 0)];
        while let Some((directory, depth)) = pending.pop() {
            for entry in std::fs::read_dir(directory)? {
                let entry = entry?;
                let path = entry.path();
                let kind = entry.file_type()?;
                if kind.is_symlink() {
                    return Err(NexoraError::InvalidPath(format!(
                        "symlink rejected: {}",
                        path.display()
                    )));
                }
                if kind.is_dir() {
                    if ![".git", "target", "node_modules", ".nexora"]
                        .iter()
                        .any(|n| entry.file_name() == *n)
                    {
                        pending.push((path, depth + 1));
                    } else {
                        let prefix = path
                            .strip_prefix(root)
                            .map_err(|e| NexoraError::InvalidPath(e.to_string()))?
                            .to_str()
                            .ok_or_else(|| {
                                NexoraError::InvalidPath("UTF-8 path required".into())
                            })?;
                        excluded.push(format!("{prefix}/"));
                    }
                } else if kind.is_file() {
                    files.push(path);
                }
            }
        }
        files.sort();
        excluded.sort();
        Ok((files, excluded))
    }
}
