use nexora_core::NodeId;
use nexora_graph::{EdgeKind, NodeKind, ResolutionState};
use nexora_indexer::{CompositionAnalyzer, CompositionResult};
use nexora_language::LanguageDefinition;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn analyze(source: &str) -> CompositionResult {
    let root = Path::new("../../templates/composition-molds");
    let host = LanguageDefinition::load(&root.join("region_carrier")).unwrap();
    let target = LanguageDefinition::load(&root.join("region_expression")).unwrap();
    CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("mixed.nxb", source)
        .unwrap()
}

#[test]
fn a_single_graph_preserves_each_units_origin_and_real_evidence() {
    let result =
        analyze("VAR outside; ~[@(outside + 2) @(render(~[text @(item + 3)]~))]~ VAR after;");
    let graph = &result.graph;
    let nodes: BTreeMap<_, _> = graph.nodes.iter().map(|n| (&n.id, n)).collect();
    assert_eq!(nodes.len(), graph.nodes.len());
    assert_eq!(
        graph
            .edges
            .iter()
            .map(|e| &e.id)
            .collect::<BTreeSet<_>>()
            .len(),
        graph.edges.len()
    );
    assert_eq!(graph.syntax_units.len(), result.units.len());
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::SyntaxUnit)
            .count(),
        result.units.len() - 1
    );
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::File)
            .count(),
        1
    );
    assert!(graph.nodes.iter().any(|n| n.name == "after"));
    for node in &graph.nodes {
        let owner = node.syntax_unit.as_ref().unwrap();
        let origin = &graph.syntax_units[owner];
        let unit = result
            .units
            .iter()
            .find(|u| u.id == origin.unit_id)
            .unwrap();
        assert_eq!(origin.language, unit.language);
        assert_eq!(origin.profile_fingerprint, unit.profile_fingerprint);
        assert_eq!(origin.context, unit.context);
        if let Some(evidence) = &node.evidence {
            assert_eq!(evidence.syntax_unit.as_ref(), Some(owner));
            let ast = unit
                .ast
                .nodes
                .iter()
                .find(|a| a.id == evidence.ast_id)
                .unwrap();
            assert_eq!(ast.span, evidence.span);
            assert_eq!(evidence.rule_version, unit.rule_version);
        }
        if let Some(expression) = &node.expression {
            for operand in &expression.operands {
                assert_eq!(nodes[operand].syntax_unit.as_ref(), Some(owner));
            }
        }
        if let Some(region) = &node.lexical_region {
            assert_eq!(region.id, node.id);
            if let Some(parent) = &region.parent {
                assert_eq!(nodes[parent].syntax_unit.as_ref(), Some(owner));
            }
        }
    }
    for edge in &graph.edges {
        assert!(nodes.contains_key(&edge.source) && nodes.contains_key(&edge.target));
        let owner = edge.evidence.syntax_unit.as_ref().unwrap();
        let origin = &graph.syntax_units[owner];
        let unit = result
            .units
            .iter()
            .find(|u| u.id == origin.unit_id)
            .unwrap();
        assert!(
            unit.ast
                .nodes
                .iter()
                .any(|a| a.id == edge.evidence.ast_id && a.span == edge.evidence.span)
        );
        if edge.kind != EdgeKind::Interprets {
            assert_eq!(
                nodes[&edge.source].syntax_unit,
                nodes[&edge.target].syntax_unit
            );
        }
    }
    let links: Vec<_> = graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Interprets)
        .collect();
    assert_eq!(links.len(), result.interpretations.len());
    for link in links {
        assert_eq!(nodes[&link.source].kind, NodeKind::LexicalRegion);
        assert_eq!(nodes[&link.target].kind, NodeKind::SyntaxUnit);
        assert_ne!(
            nodes[&link.source].syntax_unit,
            nodes[&link.target].syntax_unit
        );
    }
}

#[test]
fn scope_and_operand_ids_cannot_cross_interpretation_boundaries() {
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
        .analyze_source(
            "scope.nxb",
            "~[@(VAR local; EVAL local + 2;) @(VAR local; EVAL local + 3;)]~",
        )
        .unwrap();
    let graph = &result.graph;
    let locals: Vec<_> = graph
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Variable && n.name == "local")
        .collect();
    assert_eq!(locals.len(), 2);
    assert_ne!(locals[0].id, locals[1].id);
    assert_ne!(locals[0].scope_id, locals[1].scope_id);
    assert_ne!(locals[0].syntax_unit, locals[1].syntax_unit);
    for local in locals {
        let scope = local.scope_id.as_ref().unwrap();
        let unit_node = graph
            .nodes
            .iter()
            .find(|n| Some(&n.id) == local.syntax_unit.as_ref())
            .unwrap();
        assert_eq!(unit_node.owned_scope_id.as_ref(), Some(scope));
    }
    assert!(
        graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::References && e.resolution == ResolutionState::Resolved)
    );
}

#[test]
fn failed_units_and_opaque_coverage_survive_unification_and_roundtrip() {
    let result = analyze("~[@(x @ y) @(good + 2)]~ VAR after;");
    let graph = &result.graph;
    let failed = graph
        .syntax_units
        .iter()
        .find(|(_, u)| u.status == nexora_core::RegionStatus::Partial)
        .unwrap();
    assert!(
        graph
            .nodes
            .iter()
            .any(|n| &n.id == failed.0 && n.kind == NodeKind::SyntaxUnit && n.evidence.is_none())
    );
    assert!(graph.unit_coverage[failed.0].partial_regions > 0);
    assert!(graph.syntax_coverage["mixed.nxb"].opaque_regions > 0);
    assert!(graph.syntax_coverage["mixed.nxb"].partial_regions > 0);
    assert!(!result.complete);
    let copy: CompositionResult =
        serde_json::from_slice(&serde_json::to_vec(&result).unwrap()).unwrap();
    assert_eq!(copy.graph.nodes.len(), result.graph.nodes.len());
    assert_eq!(copy.graph.syntax_units.len(), result.units.len());
    assert!(copy.graph.nodes.iter().any(|n| n.name == "after"));
    assert!(
        copy.graph
            .nodes
            .iter()
            .all(|n| n.syntax_unit != Some(NodeId("nonexistent".into())))
    );
}

#[test]
fn graph8_profiles_keep_generated_syntax_units_reserved() {
    let root = Path::new("../../templates/composition-molds");
    let mut language = LanguageDefinition::load(&root.join("region_expression")).unwrap();
    language.graph.schema_version = 8;
    assert!(language.validate().is_ok());
    language
        .graph
        .mappings
        .insert("variable".into(), "SyntaxUnit".into());
    assert!(
        language.validate().is_err(),
        "a production cannot fabricate interpretation ownership"
    );
}

#[test]
fn storage_keeps_provenance_and_rejects_missing_unit_owners() {
    use nexora_storage::SnapshotStore;
    let result = analyze("~[@(value + 2)]~");
    let snapshot = nexora_storage::Snapshot {
        tenant_id: nexora_core::TenantId("local".into()),
        repository_id: nexora_core::RepositoryId("composed".into()),
        id: nexora_core::SnapshotId("snapshot".into()),
        graph: result.graph,
        sources: BTreeMap::from([("mixed.nxb".into(), result.source.content)]),
        diagnostics: Vec::new(),
        rule_version: result.analysis_engine,
        analysis_engine: None,
        analysis_configuration: None,
        source_coverage: Vec::new(),
    };
    let mut store = nexora_storage::MemoryStore::default();
    store.publish(snapshot.clone()).unwrap();
    assert_eq!(
        store
            .get(&snapshot.tenant_id, &snapshot.repository_id, &snapshot.id)
            .unwrap()
            .graph
            .syntax_units
            .len(),
        result.units.len()
    );
    let mut damaged = snapshot.clone();
    let key = damaged.graph.syntax_units.keys().next().unwrap().clone();
    damaged.graph.syntax_units.remove(&key);
    assert!(
        nexora_storage::MemoryStore::default()
            .publish(damaged)
            .is_err()
    );
    let mut damaged = snapshot.clone();
    damaged
        .graph
        .nodes
        .iter_mut()
        .find_map(|n| n.evidence.as_mut())
        .unwrap()
        .syntax_unit = Some(NodeId("missing".into()));
    assert!(
        nexora_storage::MemoryStore::default()
            .publish(damaged)
            .is_err()
    );
    let mut damaged = snapshot.clone();
    damaged.graph.edges[0].evidence.rule_version = "99.0.0".into();
    assert!(
        nexora_storage::MemoryStore::default()
            .publish(damaged)
            .is_err()
    );
}

#[test]
fn stored_interpretations_cannot_claim_another_file_or_wrong_target_unit() {
    let result = analyze("~[@(value + 2)]~");
    let mut graph = result.graph.clone();
    graph.edges[0].evidence.path = "other.nxb".into();
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
    let mut graph = result.graph;
    graph
        .edges
        .iter_mut()
        .find(|e| e.kind == EdgeKind::Interprets)
        .unwrap()
        .target = NodeId("file:mixed.nxb".into());
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
}

#[test]
fn provenance_cannot_be_erased_or_used_to_bind_across_isolated_units() {
    let result = analyze("VAR outside; ~[@(outside + 2)]~");
    let mut graph = result.graph.clone();
    graph
        .edges
        .iter_mut()
        .find(|e| e.kind != EdgeKind::Interprets)
        .unwrap()
        .evidence
        .syntax_unit = None;
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
    let mut graph = result.graph.clone();
    let host = graph
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Variable && n.name == "outside")
        .unwrap()
        .id
        .clone();
    graph
        .edges
        .iter_mut()
        .find(|e| e.kind == EdgeKind::References)
        .unwrap()
        .target = host;
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
    let mut graph = result.graph;
    let node = graph
        .nodes
        .iter_mut()
        .find(|n| n.evidence.is_some())
        .unwrap();
    node.syntax_unit = None;
    node.evidence.as_mut().unwrap().syntax_unit = None;
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
}
