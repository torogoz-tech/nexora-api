use crate::{LanguageDefinition, LanguageRegistry};
use nexora_core::NexoraError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisProfilePackage {
    pub format: String,
    pub schema_version: u32,
    pub key: String,
    pub version: String,
    pub name: String,
    pub language_id: String,
    pub models: Vec<LanguageDefinition>,
}
impl AnalysisProfilePackage {
    pub const MAX_BYTES: usize = 16 * 1024 * 1024;
    pub fn validate(&self) -> Result<(), NexoraError> {
        let invalid = || NexoraError::Configuration("invalid portable analysis profile".into());
        if self.format != "nexora.analysis-profile"
            || self.schema_version != 1
            || self.key.is_empty()
            || self.key.len() > 128
            || !self
                .key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            || self.name.trim().is_empty()
            || self.name.len() > 256
            || self.version.split('.').count() != 3
            || self.version.split('.').any(|p| {
                p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) || p.parse::<u32>().is_err()
            })
            || self.models.is_empty()
            || self.models.len() > 64
        {
            return Err(invalid());
        }
        if serde_json::to_vec(self).map_err(|_| invalid())?.len() > Self::MAX_BYTES {
            return Err(NexoraError::Limit("analysis profile exceeds 16 MiB".into()));
        }
        let registry = self.registry()?;
        let root = registry.get(&self.language_id).ok_or_else(invalid)?;
        let dependencies = registry.composition_dependencies(root)?;
        if dependencies.len() + 1 != self.models.len() {
            return Err(NexoraError::Configuration(
                "profile must contain exactly root and its transitive dependencies".into(),
            ));
        }
        Ok(())
    }
    pub fn registry(&self) -> Result<LanguageRegistry, NexoraError> {
        let mut registry = LanguageRegistry::default();
        for model in &self.models {
            registry.register(model.clone())?;
        }
        Ok(registry)
    }
    pub fn canonical(&self) -> Self {
        let mut value = self.clone();
        value
            .models
            .sort_by(|a, b| a.manifest.id.0.cmp(&b.manifest.id.0));
        value
    }
    pub fn checksum(&self) -> Result<String, NexoraError> {
        self.validate()?;
        let bytes = serde_json::to_vec(&self.canonical())
            .map_err(|e| NexoraError::Configuration(e.to_string()))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
    pub fn from_catalog(
        path: &std::path::Path,
        language_id: String,
        key: String,
        version: String,
        name: String,
    ) -> Result<Self, NexoraError> {
        let registry = LanguageRegistry::load(path)?;
        let root = registry
            .get(&language_id)
            .ok_or_else(|| NexoraError::Configuration("root model not found".into()))?;
        let mut models = vec![root.clone()];
        models.extend(registry.composition_dependencies(root)?);
        let value = Self {
            format: "nexora.analysis-profile".into(),
            schema_version: 1,
            key,
            version,
            name,
            language_id,
            models,
        };
        value.validate()?;
        Ok(value.canonical())
    }
}
