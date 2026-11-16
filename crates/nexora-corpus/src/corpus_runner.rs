use crate::*;
use nexora_core::{NexoraError, TenantId};
use nexora_governance::{AnalysisCoverage, EvaluationContext, PolicyEvaluator, PolicyRegistry};
use nexora_indexer::{Indexer, MemoryStore, SourceReader};
use std::{collections::BTreeMap, time::Instant};
pub struct CorpusRunner;
impl CorpusRunner {
    pub fn run(
        suite: &CorpusSuite,
        selected: Option<&str>,
        include_facts: bool,
    ) -> Result<CorpusReport, CorpusError> {
        let selected_cases: Vec<_> = suite
            .cases
            .iter()
            .filter(|c| selected.is_none_or(|id| id == c.id))
            .collect();
        if selected_cases.is_empty() {
            return Err(CorpusError::Configuration("unknown case id".into()));
        }
        let mut cases = Vec::new();
        for case in selected_cases {
            cases.push(Self::case(suite, case, include_facts)?);
        }
        let mut features = Vec::new();
        for pack in &suite.manifest.packs {
            for feature in &suite.manifest.features {
                let matching: Vec<_> = cases
                    .iter()
                    .filter(|c| c.pack == pack.id && c.feature == feature.id)
                    .collect();
                let supported = matching
                    .iter()
                    .filter(|c| c.classification == CorpusClassification::Supported)
                    .count();
                let partial = matching
                    .iter()
                    .filter(|c| c.classification == CorpusClassification::Partial)
                    .count();
                let rejected = matching
                    .iter()
                    .filter(|c| c.classification == CorpusClassification::Rejected)
                    .count();
                let passed = matching.iter().filter(|c| c.passed).count();
                let status = if matching.is_empty() {
                    "not_assessed"
                } else if passed != matching.len() {
                    "failed"
                } else if supported == matching.len() {
                    "supported_in_corpus"
                } else if partial == matching.len() {
                    "partial"
                } else if rejected == matching.len() {
                    "rejected"
                } else {
                    "mixed"
                };
                features.push(FeatureAssessment {
                    pack: pack.id.clone(),
                    feature: feature.id.clone(),
                    cases: matching.len(),
                    passed,
                    supported,
                    partial,
                    rejected,
                    status: status.into(),
                });
            }
        }
        let scores = CorpusScores::new(
            cases.iter().map(|c| c.scores.true_positives).sum(),
            cases.iter().map(|c| c.scores.false_positives).sum(),
            cases.iter().map(|c| c.scores.false_negatives).sum(),
        );
        let subset_scores = |benchmark: bool| {
            CorpusScores::new(
                cases
                    .iter()
                    .filter(|c| c.benchmark == benchmark)
                    .map(|c| c.scores.true_positives)
                    .sum(),
                cases
                    .iter()
                    .filter(|c| c.benchmark == benchmark)
                    .map(|c| c.scores.false_positives)
                    .sum(),
                cases
                    .iter()
                    .filter(|c| c.benchmark == benchmark)
                    .map(|c| c.scores.false_negatives)
                    .sum(),
            )
        };
        Ok(CorpusReport {
            conformance_scores: subset_scores(false),
            benchmark_scores: subset_scores(true),
            loading_ms: suite.loading_ms,
            schema_version: 1,
            corpus_id: suite.manifest.id.clone(),
            corpus_version: suite.manifest.version.clone(),
            corpus_fingerprint: suite.fingerprint.clone(),
            analysis_engine: nexora_indexer::ANALYSIS_ENGINE_VERSION.into(),
            governance_engine: nexora_governance::ENGINE_VERSION.into(),
            build: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .into(),
            os: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            passed: cases.iter().all(|c| c.passed),
            scores,
            cases,
            features,
        })
    }
    fn case(
        suite: &CorpusSuite,
        case: &CorpusCase,
        include_facts: bool,
    ) -> Result<CorpusCaseResult, CorpusError> {
        let total = Instant::now();
        let material = CorpusMaterialization::new(suite, case)?;
        let language = &suite.languages[&case.pack];
        let indexer = Indexer::new(language.clone());
        let setup_ms = total.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let mut pipeline = None;
        let mut snapshot = None;
        let mut reconstruction = None;
        let mut traceability = None;
        let mut sources = BTreeMap::new();
        let mut graph = nexora_graph::CodeGraph::default();
        let mut coverage = Vec::new();
        let diagnostics;
        let analysis = match case.mode {
            CorpusMode::File => match indexer.analyze_measured(&material.path) {
                Ok(measured) => {
                    pipeline = Some(measured.timings);
                    let r = measured.result;
                    diagnostics = r.diagnostics.len();
                    reconstruction = Some(
                        r.tokens.iter().map(|t| t.text.as_str()).collect::<String>()
                            == r.source.content,
                    );
                    let cst: BTreeMap<_, _> = r.cst.nodes.iter().map(|n| (&n.id, n)).collect();
                    let ast: BTreeMap<_, _> = r.ast.nodes.iter().map(|n| (&n.id, n)).collect();
                    traceability = Some(
                        cst.len() == r.cst.nodes.len()
                            && ast.len() == r.ast.nodes.len()
                            && r.ast.nodes.len() == r.cst.nodes.len()
                            && r.ast.nodes.iter().all(|n| {
                                cst.get(&n.cst_id).is_some_and(|c| {
                                    c.span == n.span && c.ast_kind == n.kind && c.name == n.name
                                })
                            })
                            && r.graph.edges.iter().all(|e| {
                                ast.get(&e.evidence.ast_id)
                                    .is_some_and(|n| n.span == e.evidence.span)
                            }),
                    );
                    sources.insert(r.source.path, r.source.content);
                    graph = r.graph;
                    if diagnostics == 0 {
                        AnalysisExpectation::Clean
                    } else {
                        AnalysisExpectation::Diagnostics
                    }
                }
                Err(NexoraError::Lexical(_)) => {
                    sources.insert(
                        material
                            .path
                            .file_name()
                            .ok_or_else(|| CorpusError::Configuration("filename required".into()))?
                            .to_string_lossy()
                            .into(),
                        SourceReader::read(&material.path)?,
                    );
                    diagnostics = 1;
                    AnalysisExpectation::LexicalError
                }
                Err(error) => return Err(error.into()),
            },
            CorpusMode::Repository => {
                let measured = indexer.index_measured(
                    &material.path,
                    TenantId("corpus".into()),
                    &mut MemoryStore::default(),
                )?;
                pipeline = Some(measured.timings);
                let s = measured.snapshot;
                diagnostics = s.diagnostics.len();
                sources = s.sources.clone();
                graph = s.graph.clone();
                coverage = s.source_coverage.clone();
                snapshot = Some(s);
                if diagnostics == 0 {
                    AnalysisExpectation::Clean
                } else {
                    AnalysisExpectation::Diagnostics
                }
            }
        };
        let analysis_elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let policy = if let Some(path) = &case.policy {
            let registry = PolicyRegistry::load(&CorpusSuite::confined(&suite.workspace, path)?)?;
            let s = snapshot
                .as_ref()
                .ok_or_else(|| CorpusError::Configuration("policy requires snapshot".into()))?;
            let mut context = EvaluationContext::new(
                s,
                &s.tenant_id,
                &s.repository_id,
                &language.manifest.id.0,
                None,
            )?;
            context.coverage = AnalysisCoverage::indexed(s);
            Some(
                PolicyEvaluator::evaluate(
                    &context,
                    &registry.policies().cloned().collect::<Vec<_>>(),
                )?
                .status,
            )
        } else {
            None
        };
        let policy_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        let (facts, invalid_evidence) = CorpusFactExtractor::extract(&graph, &sources);
        let expected = CorpusMaterialization::expected(case);
        let comparison = CorpusComparison::compare(&expected, &facts)?;
        let mut mismatches = comparison.mismatches;
        let mut check = |ok: bool, category: &str, message: &str| {
            if !ok {
                mismatches.push(CorpusMismatch {
                    category: category.into(),
                    message: message.into(),
                });
            }
        };
        check(
            analysis == case.expected.analysis,
            "analysis",
            "analysis outcome differs from annotation",
        );
        check(
            policy == case.expected.policy,
            "policy",
            "policy outcome differs from annotation",
        );
        check(invalid_evidence == 0, "evidence", "invalid source evidence");
        check(
            reconstruction != Some(false),
            "tokens",
            "tokens do not reconstruct source",
        );
        check(
            traceability != Some(false),
            "traceability",
            "CST/AST/graph traceability broken",
        );
        if let Some(expected) = &case.expected.coverage {
            let expanded: Vec<_> = (0..case.replicas)
                .flat_map(|i| {
                    expected.iter().map(move |c| {
                        let mut c = c.clone();
                        if case.replicas > 1 && case.mode == CorpusMode::Repository {
                            c.path = format!("replica-{i:04}/{}", c.path);
                        }
                        c
                    })
                })
                .collect();
            let sorted =
                |items: &[nexora_core::SourceCoverage]| -> Result<Vec<String>, serde_json::Error> {
                    let mut s = items
                        .iter()
                        .map(serde_json::to_string)
                        .collect::<Result<Vec<_>, _>>()?;
                    s.sort();
                    Ok(s)
                };
            check(
                sorted(&expanded)? == sorted(&coverage)?,
                "coverage",
                "source coverage differs from annotation",
            );
        }
        let assertions_ms = started.elapsed().as_secs_f64() * 1000.0;
        Ok(CorpusCaseResult {
            id: case.id.clone(),
            pack: case.pack.clone(),
            feature: case.feature.clone(),
            classification: case.classification,
            benchmark: case.benchmark,
            fingerprint: suite.fingerprints[&case.id].clone(),
            passed: mismatches.is_empty(),
            scores: comparison.scores,
            expected_facts: expected.len(),
            observed_facts: facts.len(),
            source_bytes: sources.values().map(String::len).sum(),
            diagnostics,
            analysis,
            policy,
            policy_expected: case.expected.policy,
            invalid_evidence,
            token_reconstruction: reconstruction,
            cst_ast_traceability: traceability,
            setup_ms,
            analysis_elapsed_ms,
            policy_ms,
            assertions_ms,
            total_ms: total.elapsed().as_secs_f64() * 1000.0,
            pipeline,
            mismatches,
            facts: if include_facts { facts } else { Vec::new() },
        })
    }
}
