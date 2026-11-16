use nexora_core::NexoraError;
use nexora_language::{LanguageDefinition, LanguageRegistry};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
pub struct LanguageSelection;
impl LanguageSelection {
    pub fn indexer(
        registry: &LanguageRegistry,
        language: LanguageDefinition,
    ) -> Result<nexora_indexer::Indexer, NexoraError> {
        if language.manifest.composition.is_none() {
            return Ok(nexora_indexer::Indexer::new(language));
        }
        let dependencies = registry.composition_dependencies(&language)?;
        nexora_indexer::Indexer::with_catalog(language, dependencies)
    }

    pub fn choose(
        registry: &LanguageRegistry,
        explicit: Option<&str>,
        paths: &[PathBuf],
    ) -> Result<LanguageDefinition, NexoraError> {
        if let Some(id) = explicit {
            return registry.get(id).cloned().ok_or_else(|| {
                NexoraError::Configuration(format!("unknown language pack {id:?}"))
            });
        }
        if registry.languages().count() == 1 {
            return registry
                .languages()
                .next()
                .cloned()
                .ok_or_else(|| NexoraError::Configuration("empty language catalog".into()));
        }
        let ids: BTreeSet<_> = paths
            .iter()
            .flat_map(|p| registry.candidates(p))
            .map(|d| d.manifest.id.0.as_str())
            .collect();
        if ids.len() != 1 {
            return Err(NexoraError::Configuration(
                "cannot infer one language scope; select --language or --language-dir <pack>"
                    .into(),
            ));
        }
        let id = ids
            .first()
            .ok_or_else(|| NexoraError::Configuration("language scope".into()))?;
        registry
            .get(id)
            .cloned()
            .ok_or_else(|| NexoraError::Configuration("language scope missing".into()))
    }
    pub fn path(
        registry: &LanguageRegistry,
        explicit: Option<&str>,
        path: &Path,
    ) -> Result<LanguageDefinition, NexoraError> {
        if explicit.is_some() || registry.languages().count() == 1 {
            return Self::choose(registry, explicit, &[]);
        }
        let path = nexora_indexer::SourceReader::validate_path(path)?;
        let paths = if path.is_file() {
            vec![path]
        } else {
            nexora_indexer::RepositoryWalker::collect(&path)?
        };
        Self::choose(registry, None, &paths)
    }
}
