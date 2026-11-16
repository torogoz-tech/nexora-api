use nexora_graph::{EdgeKind, ResolutionState};
use nexora_indexer::{CompositionAnalyzer, CompositionResult};
use nexora_language::LanguageDefinition;
use std::path::Path;
fn profiles() -> (LanguageDefinition, LanguageDefinition) {
    let root = Path::new("../../templates/composition-scopes");
    let mut host = LanguageDefinition::load(&root.join("region_carrier")).unwrap();
    let mut target = LanguageDefinition::load(&root.join("region_expression")).unwrap();
    for profile in [&mut host, &mut target] {
        profile.graph.schema_version = 10;
        let mut manifest = serde_json::to_value(&profile.manifest).unwrap();
        manifest["composition"]["schema_version"] = 5.into();
        manifest["composition"]["direct_calls"] = true.into();
        profile.manifest = serde_json::from_value(manifest).unwrap();
    }
    (host, target)
}
fn analyze(source: &str) -> CompositionResult {
    let (host, target) = profiles();
    CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("calls.nxb", source)
        .unwrap()
}
fn calls(result: &CompositionResult, state: ResolutionState) -> usize {
    result
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls && e.resolution == state)
        .count()
}
#[test]
fn direct_calls_can_resolve_across_template_and_header_contexts() {
    let result = analyze("FUNC run() {} ~[@(run())]~ <Box value={run()}/>");
    assert!(result.complete);
    assert_eq!(calls(&result, ResolutionState::Resolved), 2);
    let json = serde_json::to_value(&result.graph).unwrap();
    assert!(
        json["edges"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "CALLS")
            .all(|e| e["call_binding"].is_object())
    );
    nexora_graph::GraphIntegrityValidator::validate(&result.graph).unwrap();
}
#[test]
fn shared_mutations_block_calls_regardless_of_source_order() {
    for source in [
        "FUNC run() {} EVAL run(); ~[@(run = 1)]~",
        "FUNC run() {} ~[@(run = 1)]~ EVAL run();",
        "FUNC run() {} EVAL run(); ~[@(run++)]~",
        "FUNC run() {} EVAL run(); ~[@(missing = 1)]~",
        "FUNC run() {} EVAL run = 1; ~[@(run())]~",
    ] {
        let result = analyze(source);
        assert_eq!(calls(&result, ResolutionState::Resolved), 0, "{source}");
        assert_eq!(calls(&result, ResolutionState::Unresolved), 1, "{source}");
    }
}
#[test]
fn member_writes_do_not_reassign_the_receivers_or_other_lexical_functions() {
    let result = analyze("FUNC run() {} EVAL run(); ~[@(object.run = 1)]~");
    assert!(result.complete);
    assert_eq!(calls(&result, ResolutionState::Resolved), 1);
}
#[test]
fn group_policy_callable_kinds_and_indirect_callees_remain_explicit() {
    let result = analyze("FUNC run() {} ~[@(((run))())]~");
    assert_eq!(calls(&result, ResolutionState::Resolved), 1);
    let (host, mut target) = profiles();
    target
        .graph
        .expression_bindings
        .get_mut("expression")
        .unwrap()
        .unwrap_groups = false;
    let result = CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("groups.nxb", "FUNC run() {} ~[@(((run))())]~")
        .unwrap();
    assert_eq!(calls(&result, ResolutionState::Unresolved), 1);
    for source in ["VAR run; ~[@(run())]~", "FUNC run() {} ~[@(object.run())]~"] {
        let result = analyze(source);
        assert_eq!(calls(&result, ResolutionState::Resolved), 0);
        assert_eq!(calls(&result, ResolutionState::Unresolved), 1);
    }
}
#[test]
fn ambiguous_or_incomplete_bindings_cannot_prove_a_direct_call() {
    let result = analyze("FUNC run() {} FUNC run() {} ~[@(run())]~");
    assert_eq!(calls(&result, ResolutionState::Ambiguous), 1);
    let result = analyze("FUNC run() {} ~[@(run())]~ VAR broken");
    assert!(!result.complete);
    assert_eq!(calls(&result, ResolutionState::Resolved), 0);
}

#[test]
fn a_call_proof_cannot_override_source_callable_kinds_or_erase_unary_writes() {
    for source in [
        "VAR run; ~[@(run())]~",
        "FUNC run() {} EVAL run(); ~[@(run++)]~",
    ] {
        let mut graph = analyze(source).graph;
        let declaration = graph
            .nodes
            .iter()
            .find(|n| {
                n.name == "run"
                    && matches!(
                        n.kind,
                        nexora_graph::NodeKind::Function | nexora_graph::NodeKind::Variable
                    )
            })
            .unwrap()
            .id
            .clone();
        let variable = graph
            .nodes
            .iter()
            .find(|n| n.id == declaration)
            .unwrap()
            .kind
            == nexora_graph::NodeKind::Variable;
        if variable {
            graph
                .edges
                .iter_mut()
                .find(|e| e.kind == EdgeKind::Calls)
                .unwrap()
                .call_binding
                .as_mut()
                .unwrap()
                .callable_kinds = vec![nexora_graph::NodeKind::Variable];
        } else {
            graph
                .nodes
                .iter_mut()
                .find(|n| {
                    n.expression
                        .as_ref()
                        .is_some_and(|s| s.kind == nexora_core::ExpressionKind::Postfix)
                })
                .unwrap()
                .mutation_operand = None;
        }
        let call = graph
            .edges
            .iter_mut()
            .find(|e| e.kind == EdgeKind::Calls)
            .unwrap();
        call.target = declaration;
        call.resolution = ResolutionState::Resolved;
        call.resolution_reason = Some("unique_composed_direct_callable/v1".into());
        assert!(
            nexora_graph::GraphIntegrityValidator::validate(&graph).is_err(),
            "accepted forged call for {source}"
        );
    }
}

#[test]
fn uncertain_writes_in_isolated_units_do_not_block_host_calls() {
    let (mut host, mut target) = profiles();
    for profile in [&mut host, &mut target] {
        profile
            .manifest
            .composition
            .as_mut()
            .unwrap()
            .routes
            .get_mut("code")
            .unwrap()
            .scope
            .parent_lookup = false;
    }
    let result = CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source(
            "isolated.nxb",
            "FUNC run() {} EVAL run(); ~[@(missing = 1)]~",
        )
        .unwrap();
    assert_eq!(calls(&result, ResolutionState::Resolved), 1);
}

#[test]
fn direct_call_proofs_reject_wrong_destinations_references_and_missing_write_evidence() {
    let result = analyze("FUNC run() {} FUNC other() {} ~[@(run())]~");
    for mutation in ["target", "reference", "proof", "permission", "policy"] {
        let mut graph = result.graph.clone();
        match mutation {
            "target" => {
                let other = graph
                    .nodes
                    .iter()
                    .find(|n| n.name == "other")
                    .unwrap()
                    .id
                    .clone();
                graph
                    .edges
                    .iter_mut()
                    .find(|e| e.kind == EdgeKind::Calls)
                    .unwrap()
                    .target = other;
            }
            "reference" => {
                graph
                    .edges
                    .iter_mut()
                    .find(|e| e.kind == EdgeKind::Calls)
                    .unwrap()
                    .call_binding
                    .as_mut()
                    .unwrap()
                    .reference = None
            }
            "proof" => {
                graph
                    .edges
                    .iter_mut()
                    .find(|e| e.kind == EdgeKind::Calls)
                    .unwrap()
                    .call_binding = None
            }
            "permission" => {
                let unit = graph
                    .edges
                    .iter()
                    .find(|e| e.kind == EdgeKind::Calls)
                    .unwrap()
                    .evidence
                    .syntax_unit
                    .clone()
                    .unwrap();
                graph.syntax_units.get_mut(&unit).unwrap().direct_calls = false;
            }
            "policy" => {
                graph
                    .edges
                    .iter_mut()
                    .find(|e| e.kind == EdgeKind::Calls)
                    .unwrap()
                    .call_binding
                    .as_mut()
                    .unwrap()
                    .unwrap_groups = false
            }
            _ => unreachable!(),
        }
        assert!(
            nexora_graph::GraphIntegrityValidator::validate(&graph).is_err(),
            "accepted corrupt {mutation}"
        );
    }
    let mut graph = analyze("FUNC run() {} EVAL run(); ~[@(run = 1)]~").graph;
    graph
        .nodes
        .iter_mut()
        .find(|n| n.mutation_operand.is_some())
        .unwrap()
        .mutation_operand = None;
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
    let mut graph = analyze("FUNC run() {} EVAL run();").graph;
    graph
        .edges
        .iter_mut()
        .find(|e| e.kind == EdgeKind::Calls)
        .unwrap()
        .call_binding = None;
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
}

#[test]
fn direct_call_permissions_are_versioned_and_require_complete_expression_policies() {
    let (mut host, _) = profiles();
    host.manifest.composition.as_mut().unwrap().schema_version = 4;
    assert!(host.validate().is_err());
    let (mut host, _) = profiles();
    host.graph.schema_version = 9;
    assert!(host.validate().is_err());
    let (mut host, _) = profiles();
    host.graph.expression_bindings.clear();
    assert!(host.validate().is_err());
}

#[test]
fn call_proofs_survive_cache_reuse_and_recompute_after_a_foreign_write_edit() {
    use nexora_core::TenantId;
    use nexora_indexer::{IndexCache, Indexer, MemoryStore};
    let (host, target) = profiles();
    let indexer = Indexer::with_catalog(host, vec![target]).unwrap();
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-calls-cache-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.nxb"), "FUNC run() {} <Box action={run()}/>").unwrap();
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let tenant = TenantId("calls".into());
    let cold = indexer
        .index_incremental(&root, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    let warm = indexer
        .index_incremental(&root, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 1);
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    assert!(
        warm.snapshot
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Calls && e.resolution == ResolutionState::Resolved)
    );
    std::fs::write(
        root.join("a.nxb"),
        "FUNC run() {} <Box action={run()}/> EVAL run = 1;",
    )
    .unwrap();
    let changed = indexer
        .index_incremental(&root, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    let full = indexer
        .index(&root, tenant, &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&changed.snapshot).unwrap(),
        serde_json::to_value(&full).unwrap()
    );
    assert!(
        !full
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Calls && e.resolution == ResolutionState::Resolved)
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_reusable_call_catalog_proves_calls_from_function_templates_and_headers() {
    let root = Path::new("../../templates/composition-calls");
    let host = LanguageDefinition::load(&root.join("region_carrier")).unwrap();
    let target = LanguageDefinition::load(&root.join("region_expression")).unwrap();
    let source = std::fs::read_to_string(root.join("fixtures/complete/source.nxb")).unwrap();
    let result = CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("catalog.nxb", &source)
        .unwrap();
    assert!(result.complete);
    assert_eq!(calls(&result, ResolutionState::Resolved), 2);
    assert!(result.units.iter().all(|u| u.diagnostics.is_empty()));
}

#[test]
fn aggregate_call_validation_keeps_the_inspection_budget_independent_per_file() {
    use nexora_core::TenantId;
    use nexora_indexer::{Indexer, MemoryStore};
    let (host, target) = profiles();
    let indexer = Indexer::with_catalog(host, vec![target]).unwrap();
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-call-budget-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let statement = format!("EVAL {}run{}();", "(".repeat(10), ")".repeat(10));
    let source = format!("FUNC run() {{}} {}", statement.repeat(500));
    for index in 0..19 {
        std::fs::write(root.join(format!("{index}.nxb")), &source).unwrap();
    }
    let snapshot = indexer.index(
        &root,
        TenantId("call-budget".into()),
        &mut MemoryStore::default(),
    );
    std::fs::remove_dir_all(root).unwrap();
    let snapshot = snapshot.unwrap();
    assert_eq!(snapshot.graph.expression_policies.len(), 1);
    assert_eq!(
        snapshot
            .graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls && e.resolution == ResolutionState::Resolved)
            .count(),
        9500
    );
}

#[test]
fn shared_source_policies_cannot_be_changed_without_invalidating_their_identity() {
    let mut graph = analyze("FUNC run() {} ~[@(run())]~").graph;
    graph
        .expression_policies
        .values_mut()
        .next()
        .unwrap()
        .callable_kinds
        .clear();
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
}
