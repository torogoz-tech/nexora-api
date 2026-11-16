use crate::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
pub struct CorpusMaterialization {
    pub path: PathBuf,
    owned: Option<PathBuf>,
}
impl CorpusMaterialization {
    pub fn new(suite: &CorpusSuite, case: &CorpusCase) -> Result<Self, CorpusError> {
        let input = CorpusSuite::confined(&suite.workspace, &case.input)?;
        if case.replicas == 1 {
            return Ok(Self {
                path: input,
                owned: None,
            });
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().canonicalize()?.join(format!(
            "nexora-corpus-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| CorpusError::Configuration(e.to_string()))?
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root)?;
        let mut result = Self {
            path: root.clone(),
            owned: Some(root.clone()),
        };
        if case.mode == CorpusMode::File {
            result.path = root.join(
                input
                    .file_name()
                    .ok_or_else(|| CorpusError::Configuration("filename required".into()))?,
            );
            let source = nexora_indexer::SourceReader::read(&input)?;
            if source.len().saturating_mul(case.replicas) > 1_048_576 {
                return Err(CorpusError::Limit("expanded file".into()));
            }
            std::fs::write(&result.path, source.repeat(case.replicas))?;
        } else {
            let files = CorpusInput::files(&input, case.mode, &suite.languages[&case.pack])?;
            CorpusInput::validate_expansion(&input, &files, case.replicas)?;
            let mut bytes = 0usize;
            if files.len().saturating_mul(case.replicas) > 10_000 {
                return Err(CorpusError::Limit("expanded repository files".into()));
            }
            for i in 0..case.replicas {
                for file in &files {
                    let source = nexora_indexer::SourceReader::read(file)?;
                    bytes += source.len();
                    if bytes > 33_554_432 {
                        return Err(CorpusError::Limit("expanded repository bytes".into()));
                    }
                    let relative = file
                        .strip_prefix(&input)
                        .map_err(|e| CorpusError::Configuration(e.to_string()))?;
                    let target = root.join(format!("replica-{i:04}")).join(relative);
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(target, source)?;
                }
            }
        }
        Ok(result)
    }
    pub fn expected(case: &CorpusCase) -> Vec<CorpusFact> {
        (0..case.replicas)
            .flat_map(|i| {
                case.expected.facts.iter().map(move |f| {
                    if case.replicas > 1 && case.mode == CorpusMode::Repository {
                        f.prefixed(&format!("replica-{i:04}"))
                    } else {
                        f.clone()
                    }
                })
            })
            .collect()
    }
}
impl Drop for CorpusMaterialization {
    fn drop(&mut self) {
        if let Some(root) = &self.owned {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}
