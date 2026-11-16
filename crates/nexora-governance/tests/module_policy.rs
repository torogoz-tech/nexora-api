use nexora_core::TenantId;
use nexora_governance::*;
use nexora_indexer::{Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::path::Path;
fn evaluate(case: &str) -> EvaluationResult {
    let root = Path::new("../../policies/architecture/domain-boundary")
        .canonicalize()
        .unwrap();
    let registry = PolicyRegistry::load(&root).unwrap();
    let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    let snapshot = Indexer::new(language)
        .index(
            &root.join("fixtures").join(case),
            TenantId("local".into()),
            &mut MemoryStore::default(),
        )
        .unwrap();
    let mut context = EvaluationContext::new(
        &snapshot,
        &snapshot.tenant_id,
        &snapshot.repository_id,
        "javascript",
        None,
    )
    .unwrap();
    context.coverage = AnalysisCoverage::indexed(&snapshot);
    PolicyEvaluator::evaluate(&context, &registry.policies().cloned().collect::<Vec<_>>()).unwrap()
}
#[test]
fn domain_dependency_policy_evaluates_real_local_imports() {
    for (case, status) in [
        ("pass", EvaluationStatus::Pass),
        ("no-imports", EvaluationStatus::Pass),
        ("aliased-loader", EvaluationStatus::Unknown),
        ("fail", EvaluationStatus::Fail),
        ("mixed", EvaluationStatus::Fail),
        ("external", EvaluationStatus::Unknown),
        ("missing", EvaluationStatus::Unknown),
        ("ambiguous", EvaluationStatus::Unknown),
        ("dynamic", EvaluationStatus::Unknown),
    ] {
        let result = evaluate(case);
        assert_eq!(result.status, status, "{case}");
        assert!(!result.findings.is_empty());
        assert!(result.findings.iter().all(|f| !f.evidence.is_empty()));
    }
    let fail = evaluate("fail");
    let evidence = fail
        .findings
        .iter()
        .flat_map(|f| &f.evidence)
        .find(|e| !e.edge_ids.is_empty())
        .unwrap();
    assert_eq!(evidence.path.as_deref(), Some("domain/order.js"));
    assert!(evidence.span.is_some());
}
