use crate::{
    AnalysisProfileCandidate, CacheFragment, ExternalFileCache, FileCacheKey, IncrementalSnapshot,
    Indexer, MultiProfileIndexResult, RepositoryWalker, ReuseStatistics, SourceReader,
    StageTimings,
};
use nexora_core::{NexoraError, NodeId, RepositoryId, SnapshotId, TenantId};
use nexora_graph::{CodeGraph, CodeNode, NodeKind};
use nexora_storage::{Snapshot, SnapshotStore};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::{Duration, Instant},
};

/// Dispatches each file to one independently compiled root package and publishes once.
pub struct MultiProfileIndexer {
    candidates: Vec<AnalysisProfileCandidate>,
    by_extension: BTreeMap<String, usize>,
    automatic: bool,
    preparation: Duration,
}
impl MultiProfileIndexer {
    pub const MAX_CANDIDATES: usize = 1024;
    pub const MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024;
    pub fn new(
        candidates: Vec<AnalysisProfileCandidate>,
        automatic: bool,
    ) -> Result<Self, NexoraError> {
        let started = Instant::now();
        if candidates.is_empty() || candidates.len() > Self::MAX_CANDIDATES {
            return Err(NexoraError::Configuration(
                "analysis requires 1..1024 pinned candidates".into(),
            ));
        }
        let mut by_extension = BTreeMap::<String, usize>::new();
        let mut ids = BTreeSet::new();
        let mut package_bytes = 0usize;
        for (index, candidate) in candidates.iter().enumerate() {
            if candidate.id.is_empty() || candidate.id.len() > 128 || !ids.insert(&candidate.id) {
                return Err(NexoraError::Configuration(
                    "unique profile IDs required".into(),
                ));
            }
            let timestamp = candidate.created_at.as_bytes();
            if timestamp.len() != 27
                || timestamp.iter().enumerate().any(|(i, b)| match i {
                    4 | 7 => *b != b'-',
                    10 => *b != b'T',
                    13 | 16 => *b != b':',
                    19 => *b != b'.',
                    26 => *b != b'Z',
                    _ => !b.is_ascii_digit(),
                })
            {
                return Err(NexoraError::Configuration(
                    "canonical UTC profile import time required".into(),
                ));
            }
            let bytes = serde_json::to_vec(&candidate.package.canonical())
                .map_err(|e| NexoraError::Configuration(e.to_string()))?;
            package_bytes = package_bytes.saturating_add(bytes.len());
            if package_bytes > Self::MAX_PACKAGE_BYTES {
                return Err(NexoraError::Limit(
                    "pinned profile packages > 64 MiB".into(),
                ));
            }
            if candidate.checksum != candidate.package.checksum()? {
                return Err(NexoraError::Configuration(
                    "pinned profile checksum mismatch".into(),
                ));
            }
            let root = candidate
                .package
                .models
                .iter()
                .find(|model| model.manifest.id.0 == candidate.package.language_id)
                .ok_or_else(|| NexoraError::Configuration("root profile missing".into()))?;
            for extension in &root.manifest.extensions {
                if let Some(&previous) = by_extension.get(extension) {
                    if !automatic {
                        return Err(NexoraError::Configuration(format!(
                            "manual profiles overlap on .{extension}"
                        )));
                    }
                    if Self::preferred(candidate, &candidates[previous]) {
                        by_extension.insert(extension.clone(), index);
                    }
                } else {
                    by_extension.insert(extension.clone(), index);
                }
            }
        }
        Ok(Self {
            candidates,
            by_extension,
            automatic,
            preparation: started.elapsed(),
        })
    }
    fn preferred(
        candidate: &AnalysisProfileCandidate,
        previous: &AnalysisProfileCandidate,
    ) -> bool {
        let version = |value: &str| {
            value
                .split('.')
                .map(|v| v.parse::<u32>().expect("validated package version"))
                .collect::<Vec<_>>()
        };
        version(&candidate.package.version)
            .cmp(&version(&previous.package.version))
            .then_with(|| candidate.created_at.cmp(&previous.created_at))
            // A stable ascending ID breaks equal version/import-time ties.
            .then_with(|| previous.id.cmp(&candidate.id))
            .is_gt()
    }
    pub fn index_repository_external<S: SnapshotStore>(
        &self,
        path: &Path,
        tenant: TenantId,
        repository: RepositoryId,
        store: &mut S,
        external: &mut ExternalFileCache<'_>,
    ) -> Result<MultiProfileIndexResult, NexoraError> {
        let started = Instant::now();
        if tenant.0.is_empty()
            || !repository.0.starts_with("repo_")
            || repository.0.len() <= 5
            || repository.0.len() > 128
            || !repository
                .0
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
        {
            return Err(NexoraError::Configuration(
                "tenant and registered repository ID required".into(),
            ));
        }
        let root = SourceReader::validate_path(path)?;
        if !root.is_dir() {
            return Err(NexoraError::InvalidPath(
                "repository directory required".into(),
            ));
        }
        let (paths, excluded_directories) = RepositoryWalker::collect_with_exclusions(&root)?;
        let mut assigned = BTreeMap::<usize, Vec<(std::path::PathBuf, String)>>::new();
        let mut uncovered = Vec::new();
        for path in paths {
            let relative = path
                .strip_prefix(&root)
                .map_err(|e| NexoraError::InvalidPath(e.to_string()))?
                .to_str()
                .ok_or_else(|| NexoraError::InvalidPath("UTF-8 path required".into()))?
                .to_owned();
            if let Some(&index) = path
                .extension()
                .and_then(|e| e.to_str())
                .and_then(|e| self.by_extension.get(e))
            {
                assigned.entry(index).or_default().push((path, relative));
            } else {
                uncovered.push(relative);
            }
        }
        // No compatible model is a coverage gap, not a failed synchronization.
        // Publish the repository inventory and its uncovered paths even when
        // none of the installed profiles can analyze its source language.
        let policy = if self.automatic {
            "automatic-latest/v1"
        } else {
            "manual/v1"
        };
        // Order profiles by stable identity, independent of catalog input ordering.
        let mut groups: Vec<_> = assigned.into_iter().collect();
        groups.sort_by(|(a, _), (b, _)| self.candidates[*a].id.cmp(&self.candidates[*b].id));
        let mut graph = CodeGraph::default();
        graph.nodes.push(CodeNode {
            id: NodeId(format!("repository:{}", repository.0)),
            kind: NodeKind::Repository,
            name: repository.0.clone(),
            expression_policy: None,
            mutation_operand: None,
            syntax_unit: None,
            expression: None,
            lexical_region: None,
            evidence: None,
            scope_id: None,
            owned_scope_id: None,
            scope_policy: None,
            regions: Vec::new(),
            captures: Default::default(),
        });
        let mut sources = BTreeMap::new();
        let mut diagnostics = Vec::new();
        let mut source_coverage = Vec::new();
        let mut report_profiles = Vec::new();
        let mut skipped_files = Vec::new();
        let mut decoded_files = Vec::new();
        let mut timings = StageTimings {
            preparation_ms: self.preparation.as_secs_f64() * 1000.0,
            ..Default::default()
        };
        let mut reuse = ReuseStatistics::default();
        let mut configuration = Sha256::new();
        configuration.update(crate::ANALYSIS_ENGINE_VERSION.as_bytes());
        configuration.update(policy.as_bytes());
        for (index, files) in groups {
            let candidate = &self.candidates[index];
            let prepare_started = Instant::now();
            let registry = candidate.package.registry()?;
            let root_model = registry
                .get(&candidate.package.language_id)
                .ok_or_else(|| NexoraError::Configuration("root profile missing".into()))?;
            // A mixed graph requires explicit provenance even for non-embedded roots.
            let indexer = Indexer::with_catalog(
                root_model.clone(),
                registry.composition_dependencies(root_model)?,
            )?;
            let (plan, _) = indexer.prepared_plan()?;
            timings.preparation_ms += prepare_started.elapsed().as_secs_f64() * 1000.0;
            let file_configuration = format!(
                "{:x}",
                Sha256::digest(format!(
                    "{policy}:{}:{}:{}",
                    candidate.id,
                    candidate.checksum,
                    indexer.configuration()
                ))
            );
            configuration.update(file_configuration.as_bytes());
            let mut profile_graph = CodeGraph::default();
            let mut profile_diagnostics = Vec::new();
            let mut profile_files = Vec::new();
            for (path, relative) in files {
                let read_started = Instant::now();
                let read = match SourceReader::read_outcome(&path) {
                    Ok(read) => read,
                    Err(
                        error @ (NexoraError::Io(_)
                        | NexoraError::InvalidPath(_)
                        | NexoraError::Limit(_)),
                    ) => {
                        let reason = error.to_string();
                        skipped_files.push(
                            json!({"path":relative,"profile_id":candidate.id,"analysis_status":"unknown","reason":reason}),
                        );
                        profile_diagnostics.push(nexora_core::AnalysisDiagnostic {
                            stage: "source_read".into(),
                            path: Some(relative.clone()),
                            message: reason,
                            span: None,
                        });
                        reuse.cache_skipped_files += 1;
                        continue;
                    }
                    Err(error) => return Err(error),
                };
                timings.read_ms += read_started.elapsed().as_secs_f64() * 1000.0;
                reuse.read_files += 1;
                let (content, encoding, raw_checksum) = match read {
                    crate::SourceReadOutcome::Decoded {
                        content,
                        encoding,
                        checksum,
                        ..
                    } => (content, encoding, checksum),
                    crate::SourceReadOutcome::Skipped {
                        reason, checksum, ..
                    } => {
                        skipped_files.push(json!({"path":relative,"profile_id":candidate.id,"analysis_status":"unknown","reason":reason,"checksum":checksum}));
                        profile_diagnostics.push(nexora_core::AnalysisDiagnostic {
                            stage: "source_encoding".into(),
                            path: Some(relative.clone()),
                            message: reason,
                            span: None,
                        });
                        reuse.cache_skipped_files += 1;
                        continue;
                    }
                };
                reuse.source_bytes_read += content.len();
                let key = FileCacheKey {
                    tenant: tenant.0.clone(),
                    repository: repository.0.clone(),
                    configuration: file_configuration.clone(),
                    engine: crate::ANALYSIS_ENGINE_VERSION.into(),
                    path: relative.clone(),
                    content_hash: format!("{:x}", Sha256::digest(content.as_bytes())),
                };
                let cached = (external.read)(&key)?;
                if let Some(fragment) =
                    cached.filter(|fragment| fragment.validate_for(&key).is_ok())
                {
                    reuse.reused_files += 1;
                    reuse.retained_files += 1;
                    profile_graph.merge(fragment.graph);
                } else {
                    let computation = Instant::now();
                    let result = match indexer.analyze_content(
                        &relative,
                        content.clone(),
                        &plan,
                        &mut timings,
                    ) {
                        Ok(result) => result,
                        Err(
                            error @ (NexoraError::Lexical(_)
                            | NexoraError::Limit(_)
                            | NexoraError::Io(_)
                            | NexoraError::InvalidPath(_)),
                        ) => {
                            let reason = error.to_string();
                            skipped_files.push(json!({"path":relative,"profile_id":candidate.id,"analysis_status":"unknown","reason":reason,"checksum":raw_checksum}));
                            profile_diagnostics.push(nexora_core::AnalysisDiagnostic {
                                stage: "file_analysis".into(),
                                path: Some(relative.clone()),
                                message: reason,
                                span: None,
                            });
                            reuse.cache_skipped_files += 1;
                            continue;
                        }
                        Err(error) => return Err(error),
                    };
                    reuse.analyzed_files += 1;
                    if result.diagnostics.is_empty() {
                        let fragment = CacheFragment::new(
                            key.clone(),
                            result.graph.clone(),
                            u64::try_from(computation.elapsed().as_micros()).unwrap_or(u64::MAX),
                        )?;
                        if (external.write)(&key, &fragment)? {
                            reuse.retained_files += 1;
                        } else {
                            reuse.cache_skipped_files += 1;
                        }
                    } else {
                        reuse.cache_skipped_files += 1;
                    }
                    profile_graph.merge(result.graph);
                    profile_diagnostics.extend(result.diagnostics);
                }
                if encoding != "UTF-8" {
                    decoded_files
                        .push(json!({"path":relative,"encoding":encoding,"checksum":raw_checksum}));
                }
                sources.insert(relative.clone(), content);
                profile_files.push(relative);
            }
            let resolution_started = Instant::now();
            // Resolution stays within this root package. Cross-root semantics are not inferred.
            indexer.resolve_repository_graph(&mut profile_graph, &mut profile_diagnostics)?;
            timings.resolution_ms += resolution_started.elapsed().as_secs_f64() * 1000.0;
            source_coverage.extend(indexer.source_coverage(&profile_graph, &profile_diagnostics));
            graph.merge(profile_graph);
            diagnostics.extend(profile_diagnostics);
            report_profiles.push(json!({ "id":candidate.id, "key":candidate.package.key, "version":candidate.package.version, "name":candidate.package.name, "language_id":candidate.package.language_id, "checksum":candidate.checksum, "files":profile_files }));
        }
        let status = if skipped_files.is_empty() && diagnostics.is_empty() && uncovered.is_empty() {
            "complete"
        } else {
            "partial"
        };
        let unknown_files: BTreeSet<_> = skipped_files
            .iter()
            .filter_map(|file| {
                file.get("path")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .chain(uncovered.iter().cloned())
            .collect();
        let selection = json!({ "policy":policy, "status":status, "profiles":report_profiles, "uncovered_files":uncovered, "unknown_files":unknown_files, "excluded_files":[], "excluded_directories":excluded_directories, "skipped_files":skipped_files, "decoded_files":decoded_files });
        let selection_bytes = serde_json::to_vec(&selection)
            .map_err(|e| NexoraError::Configuration(e.to_string()))?;
        let configuration = format!("{:x}", configuration.finalize());
        let mut digest = Sha256::new();
        digest.update(configuration.as_bytes());
        for (path, source) in &sources {
            digest.update((path.len() as u64).to_le_bytes());
            digest.update(path.as_bytes());
            digest.update((source.len() as u64).to_le_bytes());
            digest.update(source.as_bytes());
        }
        digest.update(selection_bytes);
        for diagnostic in &diagnostics {
            digest.update(format!("{diagnostic:?}").as_bytes());
        }
        let snapshot = Snapshot {
            tenant_id: tenant,
            repository_id: repository,
            id: SnapshotId(format!("{:x}", digest.finalize())),
            graph,
            sources,
            diagnostics,
            rule_version: policy.into(),
            analysis_engine: Some(crate::ANALYSIS_ENGINE_VERSION.into()),
            analysis_configuration: Some(configuration),
            source_coverage,
        };
        if store
            .get(&snapshot.tenant_id, &snapshot.repository_id, &snapshot.id)
            .is_none()
        {
            store.publish(snapshot.clone())?;
        }
        timings.total_ms = started
            .elapsed()
            .saturating_add(self.preparation)
            .as_secs_f64()
            * 1000.0;
        Ok(MultiProfileIndexResult {
            index: IncrementalSnapshot {
                snapshot,
                timings,
                reuse,
            },
            selection,
        })
    }
}
