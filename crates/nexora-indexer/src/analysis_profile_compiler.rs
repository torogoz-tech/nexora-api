use crate::{AnalysisPlan, Indexer};
use nexora_core::NexoraError;
use nexora_language::AnalysisProfilePackage;
pub struct AnalysisProfileCompiler;
impl AnalysisProfileCompiler {
    pub fn compile(package: &AnalysisProfilePackage) -> Result<Indexer, NexoraError> {
        package.validate()?;
        let registry = package.registry()?;
        let root = registry
            .get(&package.language_id)
            .ok_or_else(|| NexoraError::Configuration("profile root model missing".into()))?
            .clone();
        for model in &package.models {
            AnalysisPlan::compile(model)?;
        }
        if root.manifest.composition.is_some() {
            let dependencies = registry.composition_dependencies(&root)?;
            Indexer::with_catalog(root, dependencies)
        } else {
            Ok(Indexer::new(root))
        }
    }
}
