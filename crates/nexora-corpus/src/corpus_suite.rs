use crate::*;
use nexora_governance::GovernanceIo;
use nexora_language::LanguageDefinition;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};
pub struct CorpusSuite {
    pub workspace: PathBuf,
    pub loading_ms: f64,
    pub manifest: CorpusManifest,
    pub cases: Vec<CorpusCase>,
    pub languages: BTreeMap<String, LanguageDefinition>,
    pub fingerprints: BTreeMap<String, String>,
    pub fingerprint: String,
}
impl CorpusSuite {
    pub fn confined(workspace: &Path, relative: &str) -> Result<PathBuf, CorpusError> {
        let path = Path::new(relative);
        if relative.is_empty()
            || path.is_absolute()
            || path.components().any(|c| matches!(c, Component::ParentDir))
        {
            return Err(CorpusError::Configuration(
                "relative confined path required".into(),
            ));
        }
        let path = GovernanceIo::path(&workspace.join(path))?;
        if !path.starts_with(workspace) {
            return Err(CorpusError::Configuration("path outside workspace".into()));
        }
        Ok(path)
    }
    pub fn load(manifest: &Path, workspace: &Path) -> Result<Self, CorpusError> {
        let started = std::time::Instant::now();
        let workspace = nexora_indexer::SourceReader::validate_path(workspace)?;
        let path = if manifest.is_absolute() {
            GovernanceIo::path(manifest)?
        } else {
            Self::confined(&workspace, &manifest.to_string_lossy())?
        };
        if !workspace.is_dir() || !path.starts_with(&workspace) {
            return Err(CorpusError::Configuration(
                "manifest outside workspace".into(),
            ));
        }
        let manifest: CorpusManifest = GovernanceIo::yaml(&path)?;
        if manifest.schema_version != 1
            || manifest.cases.is_empty()
            || manifest.cases.len() > 128
            || manifest.packs.is_empty()
            || manifest.packs.len() > 32
            || manifest.features.is_empty()
            || manifest.features.len() > 32
        {
            return Err(CorpusError::Configuration(
                "unsupported schema or corpus counts".into(),
            ));
        }
        let identifier = |id: &str| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        };
        if !identifier(&manifest.id) || manifest.version.len() > 64 || manifest.version.is_empty() {
            return Err(CorpusError::Configuration(
                "invalid corpus id/version".into(),
            ));
        }
        let mut languages = BTreeMap::new();
        for pack in &manifest.packs {
            let language = LanguageDefinition::load(&Self::confined(&workspace, &pack.path)?)?;
            if !identifier(&pack.id)
                || language.manifest.id.0 != pack.id
                || languages.insert(pack.id.clone(), language).is_some()
            {
                return Err(CorpusError::Configuration(
                    "invalid/duplicate pack id".into(),
                ));
            }
        }
        let mut features = BTreeSet::new();
        for feature in &manifest.features {
            if !identifier(&feature.id) || !features.insert(&feature.id) {
                return Err(CorpusError::Configuration(
                    "invalid/duplicate feature".into(),
                ));
            }
        }
        let mut cases = Vec::new();
        let mut fingerprints = BTreeMap::new();
        let mut read_bytes = 0usize;
        for entry in &manifest.cases {
            let case: CorpusCase = GovernanceIo::yaml(&Self::confined(&workspace, entry)?)?;
            if !identifier(&case.id)
                || !languages.contains_key(&case.pack)
                || !features.contains(&case.feature)
                || case.replicas == 0
                || case.replicas > 1000
                || case.expected.facts.len().saturating_mul(case.replicas) > 100_000
            {
                return Err(CorpusError::Configuration(
                    "invalid case identity, scope or replicas".into(),
                ));
            }
            let input = Self::confined(&workspace, &case.input)?;
            if (case.mode == CorpusMode::File) != input.is_file()
                || (case.mode == CorpusMode::Repository) != input.is_dir()
            {
                return Err(CorpusError::Configuration("input mode mismatch".into()));
            }
            if case.policy.is_some() != case.expected.policy.is_some()
                || (case.policy.is_some()
                    && (case.mode != CorpusMode::Repository || case.replicas != 1))
                || (case.expected.analysis == AnalysisExpectation::LexicalError
                    && (case.mode != CorpusMode::File
                        || case.classification != CorpusClassification::Rejected))
                || (case.classification == CorpusClassification::Supported
                    && (case.expected.analysis != AnalysisExpectation::Clean
                        || case.expected.policy
                            == Some(nexora_governance::EvaluationStatus::Unknown)))
            {
                return Err(CorpusError::Configuration(
                    "inconsistent expected analysis/policy/classification".into(),
                ));
            }
            for fact in &case.expected.facts {
                if fact.source().is_empty()
                    || fact.source().len() > 4096
                    || Path::new(fact.file()).is_absolute()
                    || Path::new(fact.file())
                        .components()
                        .any(|c| matches!(c, Component::ParentDir))
                {
                    return Err(CorpusError::Configuration("invalid expected fact".into()));
                }
            }
            let mut hash = Sha256::new();
            hash.update(serde_json::to_vec(&case)?);
            hash.update(format!("{:?}", languages[&case.pack]));
            let files = CorpusInput::files(&input, case.mode, &languages[&case.pack])?;
            if case.mode == CorpusMode::Repository {
                CorpusInput::validate_expansion(&input, &files, case.replicas)?;
            }
            let mut bytes = 0;
            for file in files {
                let source = nexora_indexer::SourceReader::read(&file)?;
                bytes += source.len();
                read_bytes += source.len();
                if read_bytes > 134_217_728 {
                    return Err(CorpusError::Limit("corpus input > 128 MiB".into()));
                }
                let name = if input.is_file() {
                    file.file_name().map(Path::new).unwrap_or(&file)
                } else {
                    file.strip_prefix(&input)
                        .map_err(|e| CorpusError::Configuration(e.to_string()))?
                };
                hash.update(serde_json::to_vec(&(name.to_string_lossy(), source))?);
            }
            if bytes.saturating_mul(case.replicas) > 33_554_432
                || (case.mode == CorpusMode::File
                    && bytes.saturating_mul(case.replicas) > 1_048_576)
            {
                return Err(CorpusError::Limit(
                    "expanded case exceeds source budget".into(),
                ));
            }
            if let Some(policy) = &case.policy {
                let registry =
                    nexora_governance::PolicyRegistry::load(&Self::confined(&workspace, policy)?)?;
                for policy in registry.policies() {
                    hash.update(&policy.fingerprint);
                }
            }
            if fingerprints
                .insert(case.id.clone(), format!("{:x}", hash.finalize()))
                .is_some()
            {
                return Err(CorpusError::Configuration("duplicate case id".into()));
            }
            cases.push(case);
        }
        cases.sort_by(|a, b| a.id.cmp(&b.id));
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(&manifest, &fingerprints))?)
        );
        Ok(Self {
            loading_ms: started.elapsed().as_secs_f64() * 1000.0,
            workspace,
            manifest,
            cases,
            languages,
            fingerprints,
            fingerprint,
        })
    }
    pub fn describe(&self) -> CorpusDescription {
        CorpusDescription {
            schema_version: 1,
            id: self.manifest.id.clone(),
            version: self.manifest.version.clone(),
            fingerprint: self.fingerprint.clone(),
            cases: self
                .cases
                .iter()
                .map(|c| CaseDescriptor {
                    id: c.id.clone(),
                    pack: c.pack.clone(),
                    feature: c.feature.clone(),
                    classification: c.classification,
                    benchmark: c.benchmark,
                    replicas: c.replicas,
                    fingerprint: self.fingerprints[&c.id].clone(),
                })
                .collect(),
        }
    }
}
