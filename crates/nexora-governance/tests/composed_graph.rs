mod support;
use nexora_governance::*;
use nexora_indexer::CompositionAnalyzer;
use nexora_language::LanguageDefinition;
use nexora_storage::Snapshot;
use std::path::Path;

fn snapshot() -> Snapshot {
    let root = Path::new("../../templates/composition-molds");
    let mut host = LanguageDefinition::load(&root.join("region_carrier")).unwrap();
    host.manifest
        .composition
        .as_mut()
        .unwrap()
        .routes
        .get_mut("code")
        .unwrap()
        .context = "body".into();
    let target = LanguageDefinition::load(&root.join("region_expression")).unwrap();
    let result = CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("mixed.nxb", "VAR host; ~[@(VAR interior;)]~")
        .unwrap();
    Snapshot {
        tenant_id: nexora_core::TenantId("local".into()),
        repository_id: nexora_core::RepositoryId("repo".into()),
        id: nexora_core::SnapshotId("composed".into()),
        graph: result.graph,
        sources: std::collections::BTreeMap::from([("mixed.nxb".into(), result.source.content)]),
        diagnostics: Vec::new(),
        rule_version: result.analysis_engine,
        analysis_engine: None,
        analysis_configuration: None,
        source_coverage: Vec::new(),
    }
}

#[test]
fn policies_select_the_units_language_instead_of_the_hosts_global_language() {
    let snapshot = snapshot();
    let context = EvaluationContext::new(
        &snapshot,
        &snapshot.tenant_id,
        &snapshot.repository_id,
        "region_carrier",
        None,
    )
    .unwrap();
    let mut p = support::policy();
    p.spec.target.language = Some("region_expression".into());
    p.spec.target.symbol = Some(nexora_graph::NodeKind::Variable);
    p.spec.assert = support::assertion("attribute: name\noperator: equals\nvalue: interior\n");
    let result =
        PolicyEvaluator::evaluate(&context, &[PolicyCompiler::compile(p).unwrap()]).unwrap();
    assert_eq!(result.status, EvaluationStatus::Pass);
    assert_eq!(result.findings.len(), 1);
    let id = result.findings[0].node_id.as_ref().unwrap();
    assert_eq!(
        snapshot
            .graph
            .nodes
            .iter()
            .find(|n| &n.id == id)
            .unwrap()
            .name,
        "interior"
    );
}

#[test]
fn interpretation_presence_is_queryable_without_claiming_complete_binding_coverage() {
    let snapshot = snapshot();
    let context = EvaluationContext::new(
        &snapshot,
        &snapshot.tenant_id,
        &snapshot.repository_id,
        "region_carrier",
        None,
    )
    .unwrap();
    let mut p = support::policy();
    p.spec.target.language = Some("region_carrier".into());
    p.spec.target.symbol = Some(nexora_graph::NodeKind::LexicalRegion);
    p.spec.target.filter = Some(serde_yaml_ng::from_str("name: code").unwrap());
    p.spec.assert = support::assertion("relation: INTERPRETS\ntarget:\n  symbol: syntax_unit\n");
    let result =
        PolicyEvaluator::evaluate(&context, &[PolicyCompiler::compile(p).unwrap()]).unwrap();
    assert_eq!(result.status, EvaluationStatus::Pass);
    assert!(snapshot.graph.syntax_coverage["mixed.nxb"].opaque_regions > 0);
}

#[test]
fn evaluation_rejects_a_graph_with_erased_composition_provenance() {
    let mut snapshot = snapshot();
    snapshot.graph.edges[0].evidence.syntax_unit = None;
    assert!(
        EvaluationContext::new(
            &snapshot,
            &snapshot.tenant_id,
            &snapshot.repository_id,
            "region_carrier",
            None
        )
        .is_err()
    );
}

#[test]
fn indexed_coverage_does_not_attest_opaque_composed_files_as_exhaustive() {
    let snapshot = snapshot();
    assert!(snapshot.graph.syntax_coverage["mixed.nxb"].opaque_regions > 0);
    let coverage = AnalysisCoverage::indexed(&snapshot);
    assert!(
        !coverage
            .relations
            .iter()
            .any(|c| c.relation == nexora_graph::EdgeKind::Declares && c.exhaustive)
    );
}
