use nexora_platform::PlatformError;
use std::{io::Read, path::Path};
use zeroize::Zeroizing;

pub(crate) struct LocalOperatorSecret;
impl LocalOperatorSecret {
    pub fn read(path: &Path) -> Result<Zeroizing<String>, PlatformError> {
        let invalid = || {
            PlatformError::Validation(
                "operator credential file must be private, regular and bounded",
            )
        };
        let before = std::fs::symlink_metadata(path).map_err(|_| invalid())?;
        if !before.is_file() {
            return Err(invalid());
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options.open(path).map_err(|_| invalid())?;
        let metadata = file.metadata().map_err(|_| invalid())?;
        if !metadata.is_file() {
            return Err(invalid());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if metadata.permissions().mode() & 0o077 != 0
                || before.dev() != metadata.dev()
                || before.ino() != metadata.ino()
            {
                return Err(invalid());
            }
        }
        let mut value = Zeroizing::new(String::new());
        file.take(1027)
            .read_to_string(&mut value)
            .map_err(|_| invalid())?;
        if value.ends_with('\n') {
            value.pop();
            if value.ends_with('\r') {
                value.pop();
            }
        }
        if !(12..=1024).contains(&value.len()) {
            return Err(invalid());
        }
        Ok(value)
    }
}
