mod support;
use nexora_governance::*;
use nexora_graph::ResolutionState;
use support::*;
#[test]
fn unresolved_relations_and_visit_limits_cannot_prove_absence() {
    let mut snapshot = snapshot("pass");
    snapshot.graph.edges[0].resolution = ResolutionState::Unresolved;
    let mut p = policy();
    p.spec.target.symbol = Some(nexora_graph::NodeKind::File);
    p.spec.assert = assertion("relation: DECLARES\n");
    assert_eq!(evaluate(&snapshot, p).status, EvaluationStatus::Unknown);
    let snapshot = support::snapshot("pass");
    let mut p = policy();
    p.spec.target.symbol = Some(nexora_graph::NodeKind::File);
    p.spec.assert = assertion(
        "relation: DECLARES\ntarget:\n  name: Missing\ntraversal:\n  maxDepth: 2\n  maxVisits: 1\n",
    );
    assert_eq!(evaluate(&snapshot, p).status, EvaluationStatus::Unknown);
}
#[test]
fn compilation_enforces_traversal_depth_and_complexity_limits() {
    let mut p = policy();
    p.spec.assert = assertion("relation: CALLS\ntraversal:\n  maxDepth: 33\n");
    assert!(PolicyCompiler::compile(p).is_err());
    let mut p = policy();
    p.spec.assert = assertion("all: []");
    assert!(PolicyCompiler::compile(p).is_err());
    let mut p = policy();
    let mut clause = p.spec.assert.clone();
    for _ in 0..18 {
        clause = PolicyAssertion::Group(AssertionGroup {
            all: Some(vec![clause]),
            any: None,
            none: None,
        });
    }
    p.spec.assert = clause;
    assert!(PolicyCompiler::compile(p).is_err());
}

#[test]
fn raw_snapshots_have_partial_coverage_until_explicitly_attested() {
    let snapshot = snapshot("pass");
    let mut p = policy();
    p.spec.target.symbol = Some(nexora_graph::NodeKind::File);
    p.spec.assert = assertion("none:\n  - relation: DECLARES\n    target:\n      name: Missing\n");
    let compiled = PolicyCompiler::compile(p).unwrap();
    let context = EvaluationContext::new(
        &snapshot,
        &snapshot.tenant_id,
        &snapshot.repository_id,
        "javascript",
        None,
    )
    .unwrap();
    let result = PolicyEvaluator::evaluate(&context, &[compiled]).unwrap();
    assert_eq!(result.status, EvaluationStatus::Unknown);
    assert!(result.coverage.relations.is_empty());
}

#[test]
fn synthetic_graph_cycles_terminate_and_depth_bounds_are_respected() {
    let mut snapshot = snapshot("pass");
    let forward = snapshot.graph.edges[0].clone();
    let mut back = forward.clone();
    back.id = nexora_core::EdgeId("cycle-back".into());
    back.source = forward.target.clone();
    back.target = forward.source.clone();
    snapshot.graph.edges.push(back);
    let mut p = policy();
    p.spec.target.symbol = Some(nexora_graph::NodeKind::File);
    p.spec.assert =
        assertion("relation: DECLARES\ntarget:\n  symbol: file\ntraversal:\n  maxDepth: 1\n");
    assert_eq!(
        evaluate(&snapshot, p.clone()).status,
        EvaluationStatus::Fail
    );
    p.spec.assert =
        assertion("relation: DECLARES\ntarget:\n  symbol: file\ntraversal:\n  maxDepth: 2\n");
    assert_eq!(
        evaluate(&snapshot, p.clone()).status,
        EvaluationStatus::Pass
    );
    p.spec.assert =
        assertion("relation: DECLARES\ntarget:\n  name: Missing\ntraversal:\n  maxDepth: 32\n");
    assert_eq!(evaluate(&snapshot, p).status, EvaluationStatus::Unknown);
}

#[test]
fn interpretation_relations_and_unit_selectors_compile_explicitly() {
    assert_eq!(
        PolicyCompiler::relation("INTERPRETS").unwrap(),
        nexora_graph::EdgeKind::Interprets
    );
    let mut p = policy();
    p.spec.target.symbol = Some(nexora_graph::NodeKind::LexicalRegion);
    p.spec.assert = assertion("relation: INTERPRETS\ntarget:\n  symbol: syntax_unit\n");
    assert!(PolicyCompiler::compile(p).is_ok());
}
