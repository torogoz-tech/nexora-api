use nexora_platform::PlatformError;
use nexora_platform_store::SecretGenerator;
use std::path::{Path, PathBuf};

pub(crate) struct RepositoryWorkerDirectory {
    path: PathBuf,
}
impl RepositoryWorkerDirectory {
    pub fn create(root: &Path) -> Result<Self, PlatformError> {
        let metadata = std::fs::symlink_metadata(root).map_err(|_| PlatformError::Storage)?;
        if !metadata.is_dir() || root.canonicalize().map_err(|_| PlatformError::Storage)? != root {
            return Err(PlatformError::Validation(
                "worker root must be a physical directory",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(PlatformError::Validation("worker root must be private"));
            }
        }
        let path = root.join(SecretGenerator::id("attempt")?);
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).map_err(|_| PlatformError::Storage)?;
        Ok(Self { path })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for RepositoryWorkerDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
