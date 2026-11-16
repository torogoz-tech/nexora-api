mod support;
use nexora_core::{RepositoryId, TenantId};
use nexora_governance::*;
use support::*;
#[test]
fn real_analyzer_produces_pass_fail_unknown_and_source_evidence() {
    for (case, status) in [
        ("pass", EvaluationStatus::Pass),
        ("fail", EvaluationStatus::Fail),
        ("unknown", EvaluationStatus::Unknown),
    ] {
        let snapshot = snapshot(case);
        let result = evaluate(&snapshot, policy());
        assert_eq!(result.status, status);
        assert_eq!(result.snapshot_id, snapshot.id);
        assert!(!result.findings.is_empty());
        assert!(result.findings.iter().all(|f| !f.evidence.is_empty()));
        if status != EvaluationStatus::Unknown {
            let evidence = &result.findings[0].evidence[0];
            assert_eq!(evidence.path.as_deref(), Some("source.js"));
            assert!(evidence.span.is_some());
            assert!(evidence.exhaustive);
        }
    }
}
#[test]
fn missing_calls_are_unknown_not_false_absence() {
    let snapshot = snapshot("pass");
    let mut p = policy();
    p.spec.assert = assertion("none:\n  - relation: CALLS\n");
    let result = evaluate(&snapshot, p);
    assert_eq!(result.status, EvaluationStatus::Unknown);
    assert!(!result.findings[0].evidence[0].exhaustive);
}
#[test]
fn complete_declarations_support_existence_and_negative_evidence() {
    let snapshot = snapshot("pass");
    let mut p = policy();
    p.spec.target.symbol = Some(nexora_graph::NodeKind::File);
    p.spec.assert = assertion("relation: DECLARES\ntarget:\n  name: ExampleService\n");
    assert_eq!(
        evaluate(&snapshot, p.clone()).status,
        EvaluationStatus::Pass
    );
    p.spec.assert = assertion("none:\n  - relation: DECLARES\n    target:\n      name: Missing\n");
    let result = evaluate(&snapshot, p);
    assert_eq!(result.status, EvaluationStatus::Pass);
    assert!(result.findings[0].evidence[0].exhaustive);
}
#[test]
fn no_selected_symbols_and_missing_source_evidence_are_unknown() {
    let mut snapshot = snapshot("pass");
    let mut p = policy();
    p.spec.target.filter = Some(PolicyFilter {
        name: Some("Missing".into()),
        ..Default::default()
    });
    assert_eq!(evaluate(&snapshot, p).status, EvaluationStatus::Unknown);
    snapshot.sources.clear();
    assert_eq!(
        evaluate(&snapshot, policy()).status,
        EvaluationStatus::Unknown
    );
}
#[test]
fn logical_combinations_are_evaluated_on_actual_nodes() {
    let snapshot = snapshot("pass");
    let mut p = policy();
    for (yaml, expected) in [
        (
            "all:\n  - attribute: name\n    operator: equals\n    value: ExampleService\n  - relation: CALLS\n",
            EvaluationStatus::Unknown,
        ),
        (
            "any:\n  - attribute: name\n    operator: equals\n    value: ExampleService\n  - relation: CALLS\n",
            EvaluationStatus::Pass,
        ),
        (
            "none:\n  - attribute: name\n    operator: equals\n    value: ExampleService\n  - relation: CALLS\n",
            EvaluationStatus::Fail,
        ),
    ] {
        p.spec.assert = assertion(yaml);
        assert_eq!(evaluate(&snapshot, p.clone()).status, expected);
    }
}
#[test]
fn tenant_repository_and_policy_scope_are_enforced_before_evaluation() {
    let snapshot = snapshot("pass");
    assert!(
        EvaluationContext::new(
            &snapshot,
            &TenantId("other".into()),
            &snapshot.repository_id,
            "javascript",
            None
        )
        .is_err()
    );
    assert!(
        EvaluationContext::new(
            &snapshot,
            &snapshot.tenant_id,
            &RepositoryId("other".into()),
            "javascript",
            None
        )
        .is_err()
    );
    let mut p = policy();
    p.spec.scope.tenants = vec!["other".into()];
    let context = EvaluationContext::new(
        &snapshot,
        &snapshot.tenant_id,
        &snapshot.repository_id,
        "javascript",
        None,
    )
    .unwrap();
    assert!(PolicyEvaluator::evaluate(&context, &[PolicyCompiler::compile(p).unwrap()]).is_err());
}
#[test]
fn evaluations_are_reproducible_and_independent_of_policy_version() {
    let snapshot = snapshot("pass");
    let first = evaluate(&snapshot, policy());
    assert_eq!(first.id, evaluate(&snapshot, policy()).id);
    let mut p = policy();
    p.metadata.version = PolicyVersion("1.0.1".into());
    let second = evaluate(&snapshot, p);
    assert_ne!(first.id, second.id);
    assert_eq!(first.snapshot_id, second.snapshot_id);
}
