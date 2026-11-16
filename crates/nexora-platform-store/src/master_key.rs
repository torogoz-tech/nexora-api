use crate::SecretGenerator;
use hmac::{Hmac, Mac};
use nexora_platform::PlatformError;
use sha2::Sha256;
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;
/// Protected installation material; intentionally not Debug, Clone or Serialize.
pub(crate) struct MasterKey {
    key: Zeroizing<[u8; 32]>,
}
impl MasterKey {
    pub fn path(directory: &Path, namespace: &str) -> PathBuf {
        directory.join(format!("{namespace}_master_key_v1"))
    }
    pub fn load(path: &Path) -> Result<Self, PlatformError> {
        let metadata = std::fs::symlink_metadata(path).map_err(|_| PlatformError::Storage)?;
        if !metadata.is_file() || metadata.len() != 32 {
            return Err(PlatformError::Storage);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(PlatformError::Storage);
            }
        }
        let mut bytes = Zeroizing::new(Vec::new());
        std::fs::File::open(path)
            .map_err(|_| PlatformError::Storage)?
            .take(33)
            .read_to_end(&mut bytes)
            .map_err(|_| PlatformError::Storage)?;
        if bytes.len() != 32 {
            return Err(PlatformError::Storage);
        }
        let mut key = Zeroizing::new([0u8; 32]);
        key.copy_from_slice(&bytes);
        Ok(Self { key })
    }
    pub fn create_or_load(path: &Path) -> Result<Self, PlatformError> {
        if path.exists() {
            return Self::load(path);
        }
        let parent = path.parent().ok_or(PlatformError::Storage)?;
        let temporary = parent.join(SecretGenerator::id(".platform_key_tmp")?);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| PlatformError::Storage)?;
        let result = (|| {
            let mut key = Zeroizing::new([0u8; 32]);
            getrandom::fill(key.as_mut()).map_err(|_| PlatformError::Unavailable)?;
            file.write_all(key.as_ref())
                .map_err(|_| PlatformError::Storage)?;
            file.sync_all().map_err(|_| PlatformError::Storage)?;
            match std::fs::hard_link(&temporary, path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
                Err(_) => Err(PlatformError::Storage),
            }
        })();
        drop(file);
        let _ = std::fs::remove_file(&temporary);
        result?;
        Self::load(path)
    }
    pub fn id(&self) -> String {
        SecretGenerator::checksum(self.key.as_ref())
    }
    pub fn provider_key(&self, namespace: &str) -> Result<Zeroizing<[u8; 32]>, PlatformError> {
        let hkdf = hkdf::Hkdf::<Sha256>::new(Some(b"nexora-platform-hkdf-v1"), self.key.as_ref());
        let info = format!("nexora-provider-secrets/aes256gcm/v1:{namespace}");
        let mut key = Zeroizing::new([0u8; 32]);
        hkdf.expand(info.as_bytes(), key.as_mut())
            .map_err(|_| PlatformError::Unavailable)?;
        Ok(key)
    }
    pub(crate) fn email_action_key(
        &self,
        namespace: &str,
    ) -> Result<Zeroizing<[u8; 32]>, PlatformError> {
        let hkdf =
            hkdf::Hkdf::<Sha256>::new(Some(b"nexora-email-action-hkdf-v1"), self.key.as_ref());
        let mut key = Zeroizing::new([0u8; 32]);
        hkdf.expand(
            format!("nexora-email-action-aes256gcm-v1:{namespace}").as_bytes(),
            key.as_mut(),
        )
        .map_err(|_| PlatformError::Storage)?;
        Ok(key)
    }
    pub fn fingerprint(&self, bytes: &[u8]) -> Result<String, PlatformError> {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.key.as_ref())
            .map_err(|_| PlatformError::Unavailable)?;
        mac.update(b"nexora-platform-request-v1\0");
        mac.update(bytes);
        Ok(mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    }
}
