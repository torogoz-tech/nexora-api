use crate::SourceError;
use std::{
    fs::{DirBuilder, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

pub(crate) struct SourcePrivateDirectory {
    path: PathBuf,
    retained: bool,
}
impl SourcePrivateDirectory {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn create(path: &Path) -> Result<Self, SourceError> {
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(SourceError::InvalidConfiguration);
        }
        let parent = path.parent().ok_or(SourceError::InvalidConfiguration)?;
        let metadata = std::fs::symlink_metadata(parent).map_err(|_| SourceError::Unavailable)?;
        if !metadata.is_dir() {
            return Err(SourceError::InvalidConfiguration);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(SourceError::InvalidConfiguration);
            }
        }
        let mut builder = DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                SourceError::DestinationExists
            } else {
                SourceError::Unavailable
            }
        })?;
        Ok(Self {
            path: path.into(),
            retained: false,
        })
    }
    pub fn write(&self, relative: &str, bytes: &[u8]) -> Result<(), SourceError> {
        let relative = Path::new(relative);
        if relative.as_os_str().is_empty()
            || !relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(SourceError::UnsafePath);
        }
        let path = self.path.join(relative);
        let mut builder = DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
            builder.mode(0o700);
            let mut options = OpenOptions::new();
            options.write(true).create_new(true).mode(0o600);
            builder
                .create(path.parent().ok_or(SourceError::UnsafePath)?)
                .map_err(|_| SourceError::Unavailable)?;
            options
                .open(&path)
                .map_err(|_| SourceError::Unavailable)?
                .write_all(bytes)
                .map_err(|_| SourceError::Unavailable)?;
        }
        #[cfg(not(unix))]
        {
            builder
                .create(path.parent().ok_or(SourceError::UnsafePath)?)
                .map_err(|_| SourceError::Unavailable)?;
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|_| SourceError::Unavailable)?
                .write_all(bytes)
                .map_err(|_| SourceError::Unavailable)?;
        }
        Ok(())
    }
    pub fn retain(mut self) -> PathBuf {
        self.retained = true;
        self.path.clone()
    }
}
impl Drop for SourcePrivateDirectory {
    fn drop(&mut self) {
        if !self.retained {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
