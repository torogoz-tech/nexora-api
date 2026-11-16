use crate::SourceError;
use std::path::PathBuf;
pub(crate) struct GitDiskBudget {
    pub root: PathBuf,
    pub max_bytes: u64,
    pub max_entries: usize,
}
impl GitDiskBudget {
    pub fn check(&self) -> Result<u64, SourceError> {
        let mut pending = vec![self.root.clone()];
        let mut bytes = 0u64;
        let mut count = 0usize;
        while let Some(path) = pending.pop() {
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => return Err(SourceError::Unavailable),
            };
            count = count.checked_add(1).ok_or(SourceError::LimitExceeded)?;
            if count > self.max_entries {
                return Err(SourceError::LimitExceeded);
            }
            if metadata.is_file() {
                bytes = bytes
                    .checked_add(metadata.len())
                    .ok_or(SourceError::LimitExceeded)?;
                if bytes > self.max_bytes {
                    return Err(SourceError::LimitExceeded);
                }
            } else if metadata.is_dir() {
                let directory = match std::fs::read_dir(&path) {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(_) => return Err(SourceError::Unavailable),
                };
                for entry in directory {
                    let entry = entry.map_err(|_| SourceError::Unavailable)?;
                    if count + pending.len() >= self.max_entries {
                        return Err(SourceError::LimitExceeded);
                    }
                    pending.push(entry.path());
                }
            } else {
                return Err(SourceError::UnsafePath);
            }
        }
        Ok(bytes)
    }
}
