use nexora_core::NexoraError;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Write;
pub(crate) struct CacheFragmentDigest {
    digest: Sha256,
}
impl CacheFragmentDigest {
    pub fn of<T: Serialize>(value: &T) -> Result<String, NexoraError> {
        let mut writer = Self {
            digest: Sha256::new(),
        };
        serde_json::to_writer(&mut writer, value)
            .map_err(|_| NexoraError::Storage("cache digest failed".into()))?;
        Ok(format!("{:x}", writer.digest.finalize()))
    }
}
impl Write for CacheFragmentDigest {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.digest.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
