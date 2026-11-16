use nexora_core::{SourcePosition, TenantId};
use nexora_corpus::{CorpusComparison, CorpusFact, CorpusFactExtractor, CorpusRunner, CorpusSuite};
use nexora_indexer::{Indexer, MemoryStore};
use std::path::PathBuf;
fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn suite() -> CorpusSuite {
    CorpusSuite::load(std::path::Path::new("corpus/suite.yaml"), &workspace()).unwrap()
}
#[test]
fn annotated_corpus_and_scales_pass_with_unknown_explicit() {
    let suite = suite();
    let report = CorpusRunner::run(&suite, None, false).unwrap();
    assert!(
        report.passed,
        "{}",
        serde_json::to_string_pretty(&report).unwrap()
    );
    assert_eq!(report.cases.len(), 93);
    assert_eq!(report.conformance_scores.true_positives, 248);
    assert_eq!(report.benchmark_scores.true_positives, 3969);
    assert_eq!(report.scores.false_positives, 0);
    assert_eq!(report.scores.false_negatives, 0);
    assert_eq!(
        report
            .cases
            .iter()
            .filter(|c| c.policy == Some(nexora_governance::EvaluationStatus::Unknown))
            .count(),
        6
    );
    assert_eq!(
        report
            .features
            .iter()
            .filter(|f| f.feature == "semantic.types" && f.status == "not_assessed")
            .count(),
        12
    );
    assert!(
        report
            .cases
            .iter()
            .filter(|c| c.pipeline.is_some())
            .all(|c| c.total_ms >= c.analysis_elapsed_ms)
    );
}
#[test]
fn wrong_annotations_fail_and_empty_case_selection_is_error() {
    let mut suite = suite();
    let case = &mut suite.cases[0];
    let id = case.id.clone();
    if let CorpusFact::Declaration { name, .. } = &mut case.expected.facts[0] {
        *name = "wrong".into();
    } else {
        panic!("declaration fixture expected");
    }
    let report = CorpusRunner::run(&suite, Some(&id), false).unwrap();
    assert!(!report.passed);
    assert!(report.scores.false_negatives > 0);
    assert!(report.scores.false_positives > 0);
    assert!(CorpusRunner::run(&suite, Some("not-a-case"), false).is_err());
}
#[test]
fn paths_are_confined_and_expectations_are_not_rebuilt_from_outputs() {
    let suite = suite();
    assert!(CorpusSuite::confined(&suite.workspace, "../Nexora/corpus/suite.yaml").is_err());
    assert!(CorpusSuite::confined(&suite.workspace, "/etc/passwd").is_err());
    let report = CorpusRunner::run(&suite, Some(&suite.cases[0].id), true).unwrap();
    let facts = &report.cases[0].facts;
    assert!(!facts.is_empty());
    assert_eq!(
        CorpusComparison::compare(&[], facts)
            .unwrap()
            .scores
            .precision,
        Some(0.0)
    );
}
#[test]
fn measured_and_regular_snapshots_have_identical_content_and_identity() {
    let suite = suite();
    for (id, language) in &suite.languages {
        let case = suite
            .cases
            .iter()
            .find(|c| &c.pack == id && (c.id.ends_with(".allowed") || c.id.ends_with(".nested")))
            .unwrap();
        let input = suite.workspace.join(&case.input);
        let indexer = Indexer::new(language.clone());
        let plain = indexer
            .index(&input, TenantId("test".into()), &mut MemoryStore::default())
            .unwrap();
        let measured = indexer
            .index_measured(&input, TenantId("test".into()), &mut MemoryStore::default())
            .unwrap();
        assert_eq!(
            serde_json::to_value(&plain).unwrap(),
            serde_json::to_value(&measured.snapshot).unwrap()
        );
        let times = serde_json::to_value(&measured.timings).unwrap();
        assert!(
            times
                .as_object()
                .unwrap()
                .values()
                .all(|v| v.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0))
        );
        let sum: f64 = times
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, _)| k.as_str() != "total_ms")
            .map(|(_, v)| v.as_f64().unwrap())
            .sum();
        assert!(sum <= measured.timings.total_ms + 0.001);
        let (_, invalid) = CorpusFactExtractor::extract(&plain.graph, &plain.sources);
        assert_eq!(invalid, 0);
        let mut graph = plain.graph.clone();
        graph.edges[0].evidence.span.start = SourcePosition {
            offset: 0,
            line: 99,
            column: 1,
        };
        assert!(CorpusFactExtractor::extract(&graph, &plain.sources).1 > 0);
    }
}

#[test]
fn source_attribution_is_verified_not_only_the_snippet() {
    let suite = suite();
    let case = suite
        .cases
        .iter()
        .find(|c| c.id.ends_with(".allowed") || c.id.ends_with(".nested"))
        .unwrap();
    let snapshot = Indexer::new(suite.languages[&case.pack].clone())
        .index(
            &suite.workspace.join(&case.input),
            TenantId("test".into()),
            &mut MemoryStore::default(),
        )
        .unwrap();
    let mut graph = snapshot.graph.clone();
    graph.edges[0].source = graph.edges[0].target.clone();
    assert!(CorpusFactExtractor::extract(&graph, &snapshot.sources).1 > 0);
}
#[test]
fn expression_operand_order_is_part_of_the_annotated_contract() {
    let mut suite = suite();
    let case = suite
        .cases
        .iter_mut()
        .find(|c| c.id == "brace_mold.expressions-precedence")
        .unwrap();
    let id = case.id.clone();
    let fact = case
        .expected
        .facts
        .iter_mut()
        .find(|f| matches!(f, CorpusFact::Expression { operands, .. } if operands.len() == 2))
        .unwrap();
    if let CorpusFact::Expression { operands, .. } = fact {
        operands.reverse();
    }
    let report = CorpusRunner::run(&suite, Some(&id), false).unwrap();
    assert!(!report.passed);
    assert_eq!(report.scores.false_positives, 1);
    assert_eq!(report.scores.false_negatives, 1);
}
#[test]
fn call_resolution_reasons_are_checked_independently_from_the_state() {
    let mut suite = suite();
    let case = suite
        .cases
        .iter_mut()
        .find(|c| c.id == "brace_bindings.mutation")
        .unwrap();
    let id = case.id.clone();
    let fact = case
        .expected
        .facts
        .iter_mut()
        .find(|f| {
            matches!(
                f,
                CorpusFact::Relation {
                    kind: nexora_graph::EdgeKind::Calls,
                    ..
                }
            )
        })
        .unwrap();
    if let CorpusFact::Relation {
        resolution_reason, ..
    } = fact
    {
        *resolution_reason = Some("wrong_reason".into());
    }
    let report = CorpusRunner::run(&suite, Some(&id), false).unwrap();
    assert!(!report.passed);
    assert_eq!(report.scores.false_positives, 1);
    assert_eq!(report.scores.false_negatives, 1);
}
#[test]
fn lexical_region_boundaries_and_owners_are_independently_checked() {
    let mut annotated = suite();
    let case = annotated
        .cases
        .iter_mut()
        .find(|c| c.id == "typescript.composition-simple")
        .unwrap();
    let id = case.id.clone();
    let fact = case
        .expected
        .facts
        .iter_mut()
        .find(|f| matches!(f, CorpusFact::LexicalRegion { .. }))
        .unwrap();
    if let CorpusFact::LexicalRegion { content, .. } = fact {
        *content = "wrong".into();
    }
    let report = CorpusRunner::run(&annotated, Some(&id), false).unwrap();
    assert!(!report.passed);
    assert_eq!(report.scores.false_positives, 1);
    assert_eq!(report.scores.false_negatives, 1);
    let suite = suite();
    let case = suite.cases.iter().find(|c| c.id == id).unwrap();
    let snapshot = Indexer::new(suite.languages[&case.pack].clone())
        .index(
            &suite.workspace.join(&case.input),
            TenantId("test".into()),
            &mut MemoryStore::default(),
        )
        .unwrap();
    let mut graph = snapshot.graph.clone();
    let region = graph
        .nodes
        .iter_mut()
        .find_map(|n| n.lexical_region.as_mut())
        .unwrap();
    region.parent = Some(nexora_core::NodeId("missing".into()));
    assert!(CorpusFactExtractor::extract(&graph, &snapshot.sources).1 > 0);
}
#[test]
fn captured_names_and_headers_are_part_of_the_independent_annotation() {
    let mut suite = suite();
    let case = suite
        .cases
        .iter_mut()
        .find(|c| c.id == "tsx.dynamic-selfclose")
        .unwrap();
    let id = case.id.clone();
    let fact = case
        .expected
        .facts
        .iter_mut()
        .find(|f| matches!(f,CorpusFact::LexicalRegion {captures,..} if !captures.is_empty()))
        .unwrap();
    if let CorpusFact::LexicalRegion { captures, .. } = fact {
        captures.insert("tag".into(), "wrong".into());
    }
    let r = CorpusRunner::run(&suite, Some(&id), false).unwrap();
    assert!(!r.passed);
    assert_eq!(r.scores.false_positives, 1);
    assert_eq!(r.scores.false_negatives, 1);
}
