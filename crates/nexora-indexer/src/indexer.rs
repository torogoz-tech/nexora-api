use crate::{AnalysisResult, SourceReader};
use nexora_ast::AbstractSyntaxTree;
use nexora_core::{
    AnalysisDiagnostic, NexoraError, NodeId, RepositoryId, SnapshotId, SourceFile, TenantId,
};
use nexora_graph::{CodeGraph, CodeNode, NodeKind};
use nexora_language::LanguageDefinition;
use nexora_storage::{Snapshot, SnapshotStore};
use nexora_symbols::SymbolTable;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, Mutex},
};
pub struct Indexer {
    language: LanguageDefinition,
    configuration: String,
    plan: Mutex<Option<Arc<crate::index_analysis_plan::IndexAnalysisPlan>>>,
}
impl Indexer {
    pub fn new(language: LanguageDefinition) -> Self {
        let configuration = format!(
            "{:x}",
            Sha256::digest(format!("{}:{language:?}", crate::ANALYSIS_ENGINE_VERSION))
        );
        Self {
            language,
            configuration,
            plan: Mutex::new(None),
        }
    }
    pub fn with_catalog(
        language: LanguageDefinition,
        dependencies: Vec<LanguageDefinition>,
    ) -> Result<Self, NexoraError> {
        let analyzer = crate::CompositionAnalyzer::new(language.clone(), dependencies)?;
        Ok(Self {
            configuration: analyzer.configuration_fingerprint().into(),
            language,
            plan: Mutex::new(Some(Arc::new(
                crate::index_analysis_plan::IndexAnalysisPlan::Composed(Box::new(analyzer)),
            ))),
        })
    }
    pub(crate) fn prepared_plan(
        &self,
    ) -> Result<(Arc<crate::index_analysis_plan::IndexAnalysisPlan>, bool), NexoraError> {
        let mut slot = self
            .plan
            .lock()
            .map_err(|_| NexoraError::Configuration("analysis plan lock poisoned".into()))?;
        if let Some(plan) = &*slot {
            return Ok((Arc::clone(plan), true));
        }
        if self.language.manifest.composition.is_some() {
            return Err(NexoraError::Configuration(
                "composition routes require Indexer::with_catalog and an explicit profile catalog"
                    .into(),
            ));
        }
        let plan = Arc::new(crate::index_analysis_plan::IndexAnalysisPlan::Local(
            Box::new(crate::AnalysisPlan::compile(&self.language)?),
        ));
        *slot = Some(Arc::clone(&plan));
        Ok((plan, false))
    }
    pub fn analyze(&self, path: &Path) -> Result<AnalysisResult, NexoraError> {
        Ok(self.analyze_measured(path)?.result)
    }
    pub fn analyze_measured(&self, path: &Path) -> Result<crate::MeasuredAnalysis, NexoraError> {
        let total = std::time::Instant::now();
        let mut timings = crate::StageTimings::default();
        let path = SourceReader::validate_path(path)?;
        let name = path
            .file_name()
            .and_then(|p| p.to_str())
            .ok_or_else(|| NexoraError::InvalidPath("UTF-8 filename required".into()))?;
        let started = std::time::Instant::now();
        let (plan, _) = self.prepared_plan()?;
        timings.preparation_ms += started.elapsed().as_secs_f64() * 1000.0;
        if !path
            .extension()
            .and_then(|p| p.to_str())
            .is_some_and(|e| self.language.manifest.extensions.iter().any(|x| x == e))
        {
            return Err(NexoraError::Configuration(
                "unsupported source extension".into(),
            ));
        }
        let started = std::time::Instant::now();
        let content = SourceReader::read(&path)?;
        timings.read_ms += started.elapsed().as_secs_f64() * 1000.0;
        let result = self.analyze_content(name, content, &plan, &mut timings)?;
        timings.total_ms = total.elapsed().as_secs_f64() * 1000.0;
        Ok(crate::MeasuredAnalysis { result, timings })
    }
    pub(crate) fn analyze_content(
        &self,
        name: &str,
        content: String,
        plan: &crate::index_analysis_plan::IndexAnalysisPlan,
        timings: &mut crate::StageTimings,
    ) -> Result<AnalysisResult, NexoraError> {
        let plan = match plan {
            crate::index_analysis_plan::IndexAnalysisPlan::Local(plan) => plan,
            crate::index_analysis_plan::IndexAnalysisPlan::Composed(analyzer) => {
                let started = std::time::Instant::now();
                let result = analyzer.analyze_source(name, &content)?.into_analysis()?;
                timings.composition_ms += started.elapsed().as_secs_f64() * 1000.0;
                return Ok(result);
            }
        };
        tracing::debug!(path = name, bytes = content.len(), "analyzing source");
        let started = std::time::Instant::now();
        let tokens = plan.lexer.tokenize(&content)?;
        timings.lexer_ms += started.elapsed().as_secs_f64() * 1000.0;
        let started = std::time::Instant::now();
        let cst = plan.parser.parse(&tokens);
        timings.parser_ms += started.elapsed().as_secs_f64() * 1000.0;
        let started = std::time::Instant::now();
        let ast = AbstractSyntaxTree::from_cst(&cst);
        timings.ast_ms += started.elapsed().as_secs_f64() * 1000.0;
        let started = std::time::Instant::now();
        let symbols = SymbolTable::from_ast(&ast);
        timings.symbols_ms += started.elapsed().as_secs_f64() * 1000.0;
        let started = std::time::Instant::now();
        let projection = CodeGraph::project_with_symbols(
            &ast,
            &symbols,
            name,
            &self.language,
            cst.diagnostics.is_empty(),
        )?;
        let graph = projection.graph;
        timings.graph_ms += started.elapsed().as_secs_f64() * 1000.0;
        let mut diagnostics: Vec<AnalysisDiagnostic> = cst
            .diagnostics
            .iter()
            .map(|d| AnalysisDiagnostic {
                stage: "parser".into(),
                path: Some(name.into()),
                message: d.message.clone(),
                span: Some(d.span),
            })
            .collect();
        diagnostics.extend(projection.diagnostics);
        Ok(AnalysisResult {
            source: SourceFile {
                path: name.into(),
                language: self.language.manifest.id.clone(),
                content,
            },
            tokens,
            cst,
            ast,
            symbols,
            graph,
            diagnostics,
            embedded_units: Vec::new(),
            interpretations: Vec::new(),
        })
    }
    pub(crate) fn configuration(&self) -> &str {
        &self.configuration
    }
    pub(crate) fn resolve_repository_graph(
        &self,
        graph: &mut CodeGraph,
        diagnostics: &mut Vec<AnalysisDiagnostic>,
    ) -> Result<(), NexoraError> {
        let (plan, _) = self.prepared_plan()?;
        plan.resolve_modules(graph, &self.language.manifest, diagnostics)
    }
    pub(crate) fn source_coverage(
        &self,
        graph: &CodeGraph,
        diagnostics: &[AnalysisDiagnostic],
    ) -> Vec<nexora_core::SourceCoverage> {
        let mut source_coverage = Vec::new();
        let references: std::collections::BTreeSet<_> = self
            .language
            .grammar
            .productions
            .iter()
            .filter(|p| {
                p.projection.role == nexora_core::SyntaxRole::Reference
                    && self
                        .language
                        .graph
                        .mappings
                        .get(&p.projection.ast_kind)
                        .is_some_and(|k| k == "Module")
            })
            .map(|p| p.projection.ast_kind.as_str())
            .collect();
        let relations: std::collections::BTreeSet<_> = references
            .iter()
            .filter_map(|kind| self.language.graph.reference_relations.get(*kind))
            .flatten()
            .collect();
        for node in graph.nodes.iter().filter(|n| n.kind == NodeKind::File) {
            let complete = !diagnostics
                .iter()
                .any(|d| d.path.as_deref().is_none_or(|p| p == node.name))
                && graph
                    .syntax_units
                    .get(&node.id)
                    .is_none_or(|origin| origin.status == nexora_core::RegionStatus::Parsed);
            for relation in &relations {
                source_coverage.push(nexora_core::SourceCoverage {
                    path: node.name.clone(),
                    relation: (*relation).clone(),
                    domain: "static-module-references/v1".into(),
                    complete,
                });
            }
        }
        source_coverage
    }
    /// Build a complete bounded snapshot, then publish once. Previous snapshots stay immutable.
    pub fn index<S: SnapshotStore>(
        &self,
        path: &Path,
        tenant: TenantId,
        store: &mut S,
    ) -> Result<Snapshot, NexoraError> {
        Ok(self.index_measured(path, tenant, store)?.snapshot)
    }
    pub fn index_measured<S: SnapshotStore>(
        &self,
        path: &Path,
        tenant: TenantId,
        store: &mut S,
    ) -> Result<crate::MeasuredSnapshot, NexoraError> {
        let result = self.index_impl(path, tenant, store, None, None, None)?;
        Ok(crate::MeasuredSnapshot {
            snapshot: result.snapshot,
            timings: result.timings,
        })
    }
    pub fn index_incremental<S: SnapshotStore>(
        &self,
        path: &Path,
        tenant: TenantId,
        store: &mut S,
        cache: &mut crate::IndexCache,
    ) -> Result<crate::IncrementalSnapshot, NexoraError> {
        self.index_impl(path, tenant, store, Some(cache), None, None)
    }
    pub fn index_external<S: SnapshotStore>(
        &self,
        path: &Path,
        tenant: TenantId,
        store: &mut S,
        external: &mut crate::ExternalFileCache<'_>,
    ) -> Result<crate::IncrementalSnapshot, NexoraError> {
        self.index_impl(path, tenant, store, None, Some(external), None)
    }
    /// Index a platform-registered repository with identity independent of its checkout path.
    /// Legacy entry points retain path-derived IDs and immutable historical payloads.
    pub fn index_repository<S: SnapshotStore>(
        &self,
        path: &Path,
        tenant: TenantId,
        repository: RepositoryId,
        store: &mut S,
    ) -> Result<Snapshot, NexoraError> {
        Ok(self
            .index_impl(path, tenant, store, None, None, Some(repository))?
            .snapshot)
    }
    pub fn index_repository_external<S: SnapshotStore>(
        &self,
        path: &Path,
        tenant: TenantId,
        repository: RepositoryId,
        store: &mut S,
        external: &mut crate::ExternalFileCache<'_>,
    ) -> Result<crate::IncrementalSnapshot, NexoraError> {
        self.index_impl(path, tenant, store, None, Some(external), Some(repository))
    }
    fn index_impl<S: SnapshotStore>(
        &self,
        path: &Path,
        tenant: TenantId,
        store: &mut S,
        mut cache: Option<&mut crate::IndexCache>,
        mut external: Option<&mut crate::ExternalFileCache<'_>>,
        repository: Option<RepositoryId>,
    ) -> Result<crate::IncrementalSnapshot, NexoraError> {
        let total = std::time::Instant::now();
        let mut timings = crate::StageTimings::default();
        let started = std::time::Instant::now();
        let (plan, plan_reused) = self.prepared_plan()?;
        let mut reuse = crate::ReuseStatistics {
            plan_reused,
            ..Default::default()
        };
        timings.preparation_ms += started.elapsed().as_secs_f64() * 1000.0;
        let root = SourceReader::validate_path(path)?;
        if !root.is_dir() {
            return Err(NexoraError::InvalidPath(
                "repository directory required".into(),
            ));
        }
        if tenant.0.is_empty() {
            return Err(NexoraError::Configuration("tenant required".into()));
        }
        let explicit_repository = repository.is_some();
        let repository_id = match repository {
            Some(id) => {
                if !id.0.starts_with("repo_")
                    || id.0.len() <= 5
                    || id.0.len() > 128
                    || !id
                        .0
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
                {
                    return Err(NexoraError::Configuration(
                        "registered repository ID required (repo_ prefix, 6..128 ASCII characters)"
                            .into(),
                    ));
                }
                id
            }
            None => RepositoryId(format!(
                "{:x}",
                Sha256::digest(root.to_string_lossy().as_bytes())
            )),
        };
        let scope = crate::index_scope::IndexScope {
            tenant: tenant.clone(),
            root: root.clone(),
            repository: repository_id.clone(),
            configuration: self.configuration.clone(),
        };
        let compatible = cache
            .as_ref()
            .is_some_and(|c| c.scope.as_ref() == Some(&scope));
        reuse.cache_reset = cache
            .as_ref()
            .is_some_and(|c| c.scope.is_some() && !compatible);
        let max_weight = cache.as_ref().map_or(0, |c| c.max_weight);
        let mut next = BTreeMap::new();
        let mut next_weight = 0usize;
        let mut observed = BTreeSet::new();
        let paths = crate::RepositoryWalker::collect(&root)?;
        let mut graph = CodeGraph::default();
        let mut sources = BTreeMap::new();
        let mut diagnostics = Vec::new();
        graph.nodes.push(CodeNode {
            expression_policy: None,
            mutation_operand: None,
            syntax_unit: None,
            id: NodeId(format!("repository:{}", repository_id.0)),
            kind: NodeKind::Repository,
            name: if explicit_repository {
                repository_id.0.clone()
            } else {
                root.to_string_lossy().into()
            },
            expression: None,
            lexical_region: None,
            evidence: None,
            scope_id: None,
            owned_scope_id: None,
            scope_policy: None,
            regions: Vec::new(),
            captures: Default::default(),
        });
        for path in paths {
            if !path
                .extension()
                .and_then(|p| p.to_str())
                .is_some_and(|e| self.language.manifest.extensions.iter().any(|x| x == e))
            {
                continue;
            }
            let relative = path
                .strip_prefix(&root)
                .map_err(|e| NexoraError::InvalidPath(e.to_string()))?
                .to_str()
                .ok_or_else(|| NexoraError::InvalidPath("UTF-8 path required".into()))?
                .to_owned();
            observed.insert(relative.clone());
            reuse.read_files += 1;
            let started_read = std::time::Instant::now();
            let read = SourceReader::read(&path);
            timings.read_ms += started_read.elapsed().as_secs_f64() * 1000.0;
            let mut content_hash = String::new();
            let mut external_key = None;
            let mut computation_us = 0;
            let analysis = match read {
                Ok(content) => {
                    reuse.source_bytes_read += content.len();
                    if cache.is_some() || external.is_some() {
                        content_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
                    }
                    if let Some(backend) = external.as_mut() {
                        let key = crate::FileCacheKey {
                            tenant: tenant.0.clone(),
                            repository: repository_id.0.clone(),
                            configuration: self.configuration.clone(),
                            engine: crate::ANALYSIS_ENGINE_VERSION.into(),
                            path: relative.clone(),
                            content_hash: content_hash.clone(),
                        };
                        if let Some(fragment) = (backend.read)(&key)?
                            && fragment.validate_for(&key).is_ok()
                        {
                            reuse.reused_files += 1;
                            reuse.retained_files += 1;
                            sources.insert(relative.clone(), content);
                            graph.merge(fragment.graph);
                            continue;
                        }
                        external_key = Some(key);
                    }
                    // Only pre-resolution fragments are reused; destinations are resolved anew below.
                    let hit = if compatible {
                        cache
                            .as_ref()
                            .and_then(|c| c.entries.get(&relative))
                            .filter(|f| f.hash == content_hash)
                            .cloned()
                    } else {
                        None
                    };
                    sources.insert(relative.clone(), content.clone());
                    if let Some(fragment) = hit {
                        reuse.reused_files += 1;
                        graph.merge(fragment.graph.clone());
                        diagnostics.extend(fragment.diagnostics.clone());
                        if next_weight.saturating_add(fragment.weight) <= max_weight {
                            next_weight += fragment.weight;
                            next.insert(relative.clone(), fragment);
                        } else {
                            reuse.cache_skipped_files += 1;
                        }
                        continue;
                    }
                    reuse.analyzed_files += 1;
                    let computation = std::time::Instant::now();
                    let result = self.analyze_content(&relative, content, &plan, &mut timings);
                    computation_us =
                        u64::try_from(computation.elapsed().as_micros()).unwrap_or(u64::MAX);
                    result
                }
                Err(error) => Err(error),
            };
            match analysis {
                Ok(result) => {
                    if let Some(backend) = external.as_mut() {
                        if result.diagnostics.is_empty() {
                            if let Some(key) = external_key.take() {
                                let fragment = crate::CacheFragment::new(
                                    key.clone(),
                                    result.graph.clone(),
                                    computation_us,
                                )?;
                                if (backend.write)(&key, &fragment)? {
                                    reuse.retained_files += 1;
                                } else {
                                    reuse.cache_skipped_files += 1;
                                }
                            }
                        } else {
                            reuse.cache_skipped_files += 1;
                        }
                    }
                    if cache.is_some() && result.diagnostics.is_empty() && max_weight > 0 {
                        let fragment = Arc::new(crate::cached_file::CachedFile::new(
                            content_hash,
                            result.graph.clone(),
                            result.diagnostics.clone(),
                        )?);
                        if next_weight.saturating_add(fragment.weight) <= max_weight {
                            next_weight += fragment.weight;
                            next.insert(relative.clone(), fragment);
                        } else {
                            reuse.cache_skipped_files += 1;
                        }
                    } else if cache.is_some() {
                        reuse.cache_skipped_files += 1;
                    }
                    graph.merge(result.graph);
                    diagnostics.extend(result.diagnostics);
                }
                Err(error) => {
                    if cache.is_some() {
                        reuse.cache_skipped_files += 1;
                    }
                    diagnostics.push(AnalysisDiagnostic {
                        stage: if matches!(&error, NexoraError::Io(e) if e.kind() == std::io::ErrorKind::InvalidData) { "source_encoding" } else { "indexer" }.into(),
                        path: Some(relative.clone()),
                        message: format!("{relative}: {error}"),
                        span: None,
                    });
                }
            }
        }
        let started = std::time::Instant::now();
        plan.resolve_modules(&mut graph, &self.language.manifest, &mut diagnostics)?;
        timings.resolution_ms += started.elapsed().as_secs_f64() * 1000.0;
        let source_coverage = self.source_coverage(&graph, &diagnostics);
        let mut digest = Sha256::new();
        digest.update(crate::ANALYSIS_ENGINE_VERSION.as_bytes());
        digest.update(self.configuration.as_bytes());
        for (path, content) in &sources {
            digest.update((path.len() as u64).to_le_bytes());
            digest.update(path.as_bytes());
            digest.update((content.len() as u64).to_le_bytes());
            digest.update(content.as_bytes());
        }
        for diagnostic in &diagnostics {
            digest.update(format!("{diagnostic:?}").as_bytes());
        }
        let snapshot = Snapshot {
            tenant_id: tenant,
            repository_id,
            id: SnapshotId(format!("{:x}", digest.finalize())),
            graph,
            sources,
            diagnostics,
            rule_version: self.language.manifest.version.clone(),
            analysis_engine: Some(crate::ANALYSIS_ENGINE_VERSION.into()),
            analysis_configuration: Some(self.configuration.clone()),
            source_coverage,
        };
        let started = std::time::Instant::now();
        if store
            .get(&snapshot.tenant_id, &snapshot.repository_id, &snapshot.id)
            .is_none()
        {
            store.publish(snapshot.clone())?;
        }
        timings.publication_ms += started.elapsed().as_secs_f64() * 1000.0;
        if let Some(cache) = cache.as_mut() {
            reuse.removed_files = if compatible {
                cache
                    .entries
                    .keys()
                    .filter(|p| !observed.contains(*p))
                    .count()
            } else {
                0
            };
            reuse.retained_files = next.len();
            reuse.cache_weight_bytes = next_weight;
            cache.scope = Some(scope);
            cache.entries = next;
            cache.weight = next_weight;
        }
        timings.total_ms = total.elapsed().as_secs_f64() * 1000.0;
        Ok(crate::IncrementalSnapshot {
            snapshot,
            timings,
            reuse,
        })
    }
}
