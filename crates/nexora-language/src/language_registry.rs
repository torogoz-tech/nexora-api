use crate::LanguageDefinition;
use nexora_core::NexoraError;
use std::{collections::BTreeMap, path::Path};
#[derive(Default)]
pub struct LanguageRegistry {
    definitions: BTreeMap<String, LanguageDefinition>,
}
impl LanguageRegistry {
    pub fn load(path: &Path) -> Result<Self, NexoraError> {
        if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(NexoraError::InvalidPath("language catalog symlink".into()));
        }
        let mut registry = Self::default();
        if path.join("language.yaml").is_file() {
            registry.register(LanguageDefinition::load(path)?)?;
        } else {
            let mut children = Vec::new();
            for (count, entry) in std::fs::read_dir(path)?.enumerate() {
                if count >= 256 {
                    return Err(NexoraError::Limit("language catalog entries".into()));
                }
                let entry = entry?;
                if entry.file_type()?.is_symlink() {
                    return Err(NexoraError::InvalidPath("language package symlink".into()));
                }
                if entry.file_type()?.is_dir() && entry.path().join("language.yaml").is_file() {
                    children.push(entry.path());
                }
            }
            children.sort();
            for child in children {
                registry.register(LanguageDefinition::load(&child)?)?;
            }
        }
        if registry.definitions.is_empty() {
            return Err(NexoraError::Configuration("no language packs found".into()));
        }
        Ok(registry)
    }
    pub fn get(&self, id: &str) -> Option<&LanguageDefinition> {
        self.definitions.get(id)
    }
    pub fn register(&mut self, definition: LanguageDefinition) -> Result<(), NexoraError> {
        definition.validate()?;
        if self.definitions.contains_key(&definition.manifest.id.0) {
            return Err(NexoraError::Configuration("duplicate language id".into()));
        }
        self.definitions
            .insert(definition.manifest.id.0.clone(), definition);
        Ok(())
    }
    /// Return only the declared transitive dependencies; references back to the root
    /// or an already visited profile do not expand the catalog again.
    pub fn composition_dependencies(
        &self,
        root: &LanguageDefinition,
    ) -> Result<Vec<LanguageDefinition>, NexoraError> {
        let mut known = std::collections::BTreeSet::from([root.manifest.id.0.clone()]);
        let mut pending = vec![root];
        let mut dependencies = BTreeMap::new();
        while let Some(profile) = pending.pop() {
            if let Some(composition) = &profile.manifest.composition {
                for route in composition
                    .routes
                    .values()
                    .chain(composition.header_routes.values())
                {
                    if !known.insert(route.language.0.clone()) {
                        continue;
                    }
                    if known.len() > 64 {
                        return Err(NexoraError::Limit(
                            "composition dependencies > 64 profiles".into(),
                        ));
                    }
                    let target = self.get(&route.language.0).ok_or_else(|| {
                        NexoraError::Configuration(format!(
                            "missing composition dependency {}",
                            route.language.0
                        ))
                    })?;
                    dependencies.insert(route.language.0.clone(), target.clone());
                    pending.push(target);
                }
            }
        }
        Ok(dependencies.into_values().collect())
    }
    pub fn candidates(&self, path: &Path) -> Vec<&LanguageDefinition> {
        let Some(extension) = path.extension().and_then(|s| s.to_str()) else {
            return Vec::new();
        };
        self.definitions
            .values()
            .filter(|d| d.manifest.extensions.iter().any(|e| e == extension))
            .collect()
    }
    pub fn detect(&self, path: &Path) -> Option<&LanguageDefinition> {
        let candidates = self.candidates(path);
        match candidates.as_slice() {
            [only] => Some(*only),
            _ => None,
        }
    }
    pub fn languages(&self) -> impl Iterator<Item = &LanguageDefinition> {
        self.definitions.values()
    }
}
