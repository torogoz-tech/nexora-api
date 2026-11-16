use crate::RepositoryWorkerProfile;
use nexora_platform::PlatformError;
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf};

/// Trusted operator configuration, never deserialized from HTTP or MCP requests.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryWorkerSettings {
    pub id: String,
    pub source_root: PathBuf,
    pub git_binary: PathBuf,
    pub profiles: BTreeMap<String, RepositoryWorkerProfile>,
}
impl RepositoryWorkerSettings {
    pub fn load(path: &std::path::Path) -> Result<Self, PlatformError> {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|_| PlatformError::Storage)?
            .take(1_048_577)
            .read_to_end(&mut bytes)
            .map_err(|_| PlatformError::Storage)?;
        if bytes.len() > 1_048_576 {
            return Err(PlatformError::Validation(
                "worker configuration exceeds limit",
            ));
        }
        let settings: Self = serde_json::from_slice(&bytes)
            .map_err(|_| PlatformError::Validation("invalid worker configuration"))?;
        settings.validate()?;
        Ok(settings)
    }
    pub fn validate(&self) -> Result<(), PlatformError> {
        let invalid = || PlatformError::Validation("invalid repository worker configuration");
        if self.id.is_empty()
            || self.id.len() > 128
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            || !self.source_root.is_absolute()
            || self.source_root.parent().is_none()
            || self.source_root.canonicalize().map_err(|_| invalid())? != self.source_root
            || !self.git_binary.is_absolute()
            || !self.git_binary.is_file()
            || self.profiles.len() > 10_000
        {
            return Err(invalid());
        }
        let metadata = std::fs::symlink_metadata(&self.source_root).map_err(|_| invalid())?;
        if !metadata.is_dir() {
            return Err(invalid());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(invalid());
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        return Err(PlatformError::Validation("worker host is unsupported"));
        for (repository, profile) in &self.profiles {
            if repository.is_empty()
                || repository.len() > 128
                || !profile.directory.is_absolute()
                || !profile.directory.is_dir()
                || profile.language_id.is_empty()
                || profile.language_id.len() > 128
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
