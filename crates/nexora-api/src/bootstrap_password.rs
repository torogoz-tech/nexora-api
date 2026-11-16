use nexora_platform::PlatformError;
use std::{io::Read, path::Path};
pub struct BootstrapPassword;
impl BootstrapPassword {
    pub fn read(path: &Path) -> Result<String, PlatformError> {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|_| PlatformError::Validation("password file unavailable"))?;
        if !metadata.is_file() {
            return Err(PlatformError::Validation(
                "password file must be a regular file",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(PlatformError::Validation(
                    "password file must deny group and other access",
                ));
            }
        }
        let mut value = String::new();
        std::fs::File::open(path)
            .map_err(|_| PlatformError::Validation("password file unavailable"))?
            .take(1027)
            .read_to_string(&mut value)
            .map_err(|_| PlatformError::Validation("password file must contain UTF-8"))?;
        if value.ends_with('\n') {
            value.pop();
            if value.ends_with('\r') {
                value.pop();
            }
        }
        if value.len() < 12 || value.len() > 1024 {
            return Err(PlatformError::Validation(
                "password must contain 12..1024 bytes",
            ));
        }
        Ok(value)
    }
}
