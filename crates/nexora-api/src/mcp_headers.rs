use nexora_platform::PlatformError;
use std::{io::Read, path::Path};
use zeroize::Zeroizing;

/// Credential-protocol output for a locally configured MCP client; never a diagnostic.
pub struct McpHeaders;
impl McpHeaders {
    pub fn read(path: &Path) -> Result<Zeroizing<String>, PlatformError> {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|_| PlatformError::Validation("API key file unavailable"))?;
        if !metadata.is_file() {
            return Err(PlatformError::Validation(
                "API key file must be a regular file",
            ));
        }
        let file = std::fs::File::open(path)
            .map_err(|_| PlatformError::Validation("API key file unavailable"))?;
        let metadata = file
            .metadata()
            .map_err(|_| PlatformError::Validation("API key file unavailable"))?;
        if !metadata.is_file() {
            return Err(PlatformError::Validation(
                "API key file must be a regular file",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(PlatformError::Validation(
                    "API key file must deny group and other access",
                ));
            }
        }
        let mut key = Zeroizing::new(String::new());
        file.take(1027)
            .read_to_string(&mut key)
            .map_err(|_| PlatformError::Validation("API key file must contain UTF-8"))?;
        if key.ends_with('\n') {
            key.pop();
            if key.ends_with('\r') {
                key.pop();
            }
        }
        if !(12..=1024).contains(&key.len()) || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err(PlatformError::Validation(
                "API key must be one bounded nonempty token",
            ));
        }
        let authorization = Zeroizing::new(format!("Bearer {}", key.as_str()));
        let encoded = Zeroizing::new(
            serde_json::to_string(authorization.as_str()).map_err(|_| PlatformError::Storage)?,
        );
        Ok(Zeroizing::new(format!(
            "{{\"Authorization\":{}}}",
            encoded.as_str()
        )))
    }
}
