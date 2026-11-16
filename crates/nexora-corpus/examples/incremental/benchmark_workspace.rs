use nexora_core::NexoraError;
use nexora_corpus::{CorpusCase, CorpusError, CorpusInput, CorpusSuite};
use nexora_indexer::SourceReader;
use std::path::PathBuf;
/// An owned source copy; benchmarks must never edit registered fixtures.
pub struct BenchmarkWorkspace {
    pub path: PathBuf,
}
impl BenchmarkWorkspace {
    pub fn new(suite: &CorpusSuite, case: &CorpusCase) -> Result<Self, CorpusError> {
        let material = nexora_corpus::CorpusMaterialization::new(suite, case)?;
        let files = CorpusInput::files(&material.path, case.mode, &suite.languages[&case.pack])?;
        let root = std::env::temp_dir().canonicalize()?.join(format!(
            "nexora-incremental-bench-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| NexoraError::Configuration(e.to_string()))?
                .as_nanos()
        ));
        std::fs::create_dir(&root)?;
        let owned = Self { path: root };
        for file in files {
            let relative = if case.mode == nexora_corpus::CorpusMode::File {
                std::path::Path::new(
                    file.file_name()
                        .ok_or_else(|| NexoraError::InvalidPath("filename required".into()))?,
                )
            } else {
                file.strip_prefix(&material.path)
                    .map_err(|e| NexoraError::InvalidPath(e.to_string()))?
            };
            let target = owned.path.join(relative);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(target, SourceReader::read(&file)?)?;
        }
        Ok(owned)
    }
}
impl Drop for BenchmarkWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
