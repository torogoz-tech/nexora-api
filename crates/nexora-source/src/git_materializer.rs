use crate::{
    GitEntryKind, GitObjectFormat, GitObjectReader, SourceError, SourceLimits,
    SourceMaterialization, source_private_directory::SourcePrivateDirectory,
};
use std::path::Path;
pub struct GitMaterializer;
impl GitMaterializer {
    pub fn materialize(
        reader: &GitObjectReader,
        bare: &Path,
        commit: &str,
        format: GitObjectFormat,
        destination: &Path,
        limits: &SourceLimits,
    ) -> Result<SourceMaterialization, SourceError> {
        let files = reader.tree(bare, commit, format, limits)?;
        if files.iter().any(|entry| {
            !matches!(
                entry.kind,
                GitEntryKind::File | GitEntryKind::ExecutableFile
            )
        }) {
            return Err(SourceError::UnsupportedEntry);
        }
        let total_bytes = files.iter().try_fold(0u64, |total, entry| {
            total
                .checked_add(entry.size.ok_or(SourceError::InvalidObject)?)
                .ok_or(SourceError::LimitExceeded)
        })?;
        let batch = reader.blobs(bare, &files, format, limits)?;
        let directory = SourcePrivateDirectory::create(destination)?;
        for (index, entry) in files.iter().enumerate() {
            directory.write(
                &entry.path,
                batch.get(index).ok_or(SourceError::InvalidObject)?,
            )?;
        }
        Ok(SourceMaterialization {
            root: directory.retain(),
            commit: commit.into(),
            files,
            total_bytes,
        })
    }
}
