use crate::{CorpusError, CorpusMode};
use nexora_language::LanguageDefinition;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
/// Select the same source extensions as the indexer; never interpret other files.
pub struct CorpusInput;
impl CorpusInput {
    pub fn files(
        input: &Path,
        mode: CorpusMode,
        language: &LanguageDefinition,
    ) -> Result<Vec<PathBuf>, CorpusError> {
        let candidates = if mode == CorpusMode::File {
            vec![input.to_owned()]
        } else {
            nexora_indexer::RepositoryWalker::collect(input)?
        };
        let files: Vec<_> = candidates
            .into_iter()
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| language.manifest.extensions.iter().any(|x| x == e))
            })
            .collect();
        if files.is_empty() {
            return Err(CorpusError::Configuration(
                "no sources for selected pack".into(),
            ));
        }
        Ok(files)
    }
    pub fn validate_expansion(
        input: &Path,
        files: &[PathBuf],
        replicas: usize,
    ) -> Result<(), CorpusError> {
        if replicas == 1 {
            return Ok(());
        }
        let mut directories = BTreeSet::new();
        for file in files {
            let relative = file
                .strip_prefix(input)
                .map_err(|e| CorpusError::Configuration(e.to_string()))?;
            let parent = relative
                .parent()
                .ok_or_else(|| CorpusError::Configuration("source parent missing".into()))?;
            if parent.components().count() + 1 > 64 {
                return Err(CorpusError::Limit("expanded repository depth > 64".into()));
            }
            for ancestor in parent.ancestors().take_while(|p| !p.as_os_str().is_empty()) {
                directories.insert(ancestor.to_owned());
            }
        }
        if (files.len() + directories.len() + 1).saturating_mul(replicas) > 10_000 {
            return Err(CorpusError::Limit(
                "expanded repository entries > 10000".into(),
            ));
        }
        Ok(())
    }
}
