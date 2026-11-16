use nexora_core::NexoraError;
use std::path::{Path, PathBuf};
pub struct SourceReader;
impl SourceReader {
    pub fn validate_path(path: &Path) -> Result<PathBuf, NexoraError> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let mut current = PathBuf::new();
        for component in absolute.components() {
            current.push(component.as_os_str());
            if std::fs::symlink_metadata(&current)?
                .file_type()
                .is_symlink()
            {
                return Err(NexoraError::InvalidPath(format!(
                    "symlink rejected: {}",
                    current.display()
                )));
            }
        }
        Ok(absolute.canonicalize()?)
    }
    pub fn read(path: &Path) -> Result<String, NexoraError> {
        match Self::read_outcome(path)? {
            crate::SourceReadOutcome::Decoded { content, .. } => Ok(content),
            crate::SourceReadOutcome::Skipped {
                reason, checksum, ..
            } => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{reason}; raw SHA-256 {checksum}"),
            )
            .into()),
        }
    }
    pub fn read_outcome(path: &Path) -> Result<crate::SourceReadOutcome, NexoraError> {
        Self::validate_path(path)?;
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.is_file() {
            return Err(NexoraError::InvalidPath(
                "regular source file required".into(),
            ));
        }
        let bytes = std::fs::read(path)?;
        Ok(crate::source_text_decoder::SourceTextDecoder::decode(
            &bytes,
        ))
    }
}
