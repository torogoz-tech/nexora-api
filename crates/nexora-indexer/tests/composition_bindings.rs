use nexora_core::{DeclarationVisibility, ShadowingPolicy};
use nexora_graph::{EdgeKind, ResolutionState};
use nexora_indexer::{CompositionAnalyzer, CompositionResult};
use nexora_language::LanguageDefinition;
use std::path::Path;

fn profiles(bridged: bool) -> (LanguageDefinition, LanguageDefinition) {
    let root = Path::new("../../templates/composition-markup");
    let mut host = LanguageDefinition::load(&root.join("region_carrier")).unwrap();
    let mut target = LanguageDefinition::load(&root.join("region_expression")).unwrap();
    if bridged {
        for profile in [&mut host, &mut target] {
            profile.graph.schema_version = 9;
            let composition = profile.manifest.composition.as_mut().unwrap();
            composition.schema_version = 4;
            for route in composition
                .routes
                .values_mut()
                .chain(composition.header_routes.values_mut())
            {
                route.scope.parent_lookup = true;
            }
        }
    }
    (host, target)
}
fn analyze(
    host: LanguageDefinition,
    target: LanguageDefinition,
    source: &str,
) -> CompositionResult {
    CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("bindings.nxb", source)
        .unwrap()
}
fn references(result: &CompositionResult, state: ResolutionState) -> usize {
    result
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::References && e.resolution == state)
        .count()
}

#[test]
fn a_route_can_explicitly_resolve_host_names_while_legacy_routes_remain_isolated() {
    let source = "VAR host; ~[@(host + 1)]~ READ host;";
    let (host, target) = profiles(false);
    let isolated = analyze(host, target, source);
    assert_eq!(references(&isolated, ResolutionState::Resolved), 1);
    assert_eq!(references(&isolated, ResolutionState::Unresolved), 1);
    let (host, target) = profiles(true);
    let result = analyze(host, target, source);
    assert!(result.complete);
    assert_eq!(references(&result, ResolutionState::Resolved), 2);
    assert_eq!(references(&result, ResolutionState::Unresolved), 0);
    assert!(
        result
            .graph
            .edges
            .iter()
            .any(|e| format!("{:?}", e.kind) == "ScopeBridge")
    );
    nexora_graph::GraphIntegrityValidator::validate(&result.graph).unwrap();
}

#[test]
fn embedded_references_use_function_parameters_and_locals_from_the_actual_containing_scope() {
    let (host, target) = profiles(true);
    let result = analyze(
        host,
        target,
        "VAR name; FUNC f(name) { VAR local; EVAL ~[@(name + local)]~; }",
    );
    assert!(result.complete);
    assert_eq!(references(&result, ResolutionState::Resolved), 2);
    for edge in result
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::References)
    {
        let target = result
            .graph
            .nodes
            .iter()
            .find(|n| n.id == edge.target)
            .unwrap();
        assert!(matches!(
            target.kind,
            nexora_graph::NodeKind::Parameter | nexora_graph::NodeKind::Variable
        ));
        if target.name == "name" {
            assert_eq!(target.kind, nexora_graph::NodeKind::Parameter);
        }
    }
}

#[test]
fn parent_visibility_uses_original_positions_instead_of_hoisting_every_declaration() {
    let (mut host, target) = profiles(true);
    host.grammar.file_scope.as_mut().unwrap().visibility = DeclarationVisibility::AfterDeclaration;
    let result = analyze(host.clone(), target.clone(), "~[@(late)]~ VAR late;");
    assert_eq!(references(&result, ResolutionState::Unresolved), 1);
    let result = analyze(host, target, "VAR early; ~[@(early)]~");
    assert_eq!(references(&result, ResolutionState::Resolved), 1);
}

#[test]
fn collect_shadowing_reconsiders_a_previously_local_unique_reference() {
    let (mut host, mut target) = profiles(true);
    for profile in [&mut host, &mut target] {
        let route = profile
            .manifest
            .composition
            .as_mut()
            .unwrap()
            .routes
            .get_mut("code")
            .unwrap();
        route.context = "body".into();
        route.scope.shadowing = ShadowingPolicy::Collect;
    }
    let result = analyze(host, target, "VAR name; ~[@(VAR name; READ name;)]~");
    assert!(result.complete);
    assert_eq!(references(&result, ResolutionState::Ambiguous), 1);
    assert_eq!(references(&result, ResolutionState::Resolved), 0);
    nexora_graph::GraphIntegrityValidator::validate(&result.graph).unwrap();
}

#[test]
fn repeated_discovery_through_a_header_uses_one_nearest_parent_chain() {
    let (host, target) = profiles(true);
    let result = analyze(host, target, "VAR host; <Box count={host}/>");
    assert!(result.complete);
    assert_eq!(result.units.len(), 3);
    assert_eq!(references(&result, ResolutionState::Resolved), 1);
    let bridges: Vec<_> = result
        .graph
        .edges
        .iter()
        .filter(|e| format!("{:?}", e.kind) == "ScopeBridge")
        .collect();
    assert_eq!(bridges.len(), 2);
    let expression_owner = result
        .graph
        .syntax_units
        .iter()
        .find(|(_, u)| u.context.as_deref() == Some("expression"))
        .unwrap()
        .0;
    let incoming = bridges
        .iter()
        .find(|e| &e.target == expression_owner)
        .unwrap();
    let parent = result
        .graph
        .nodes
        .iter()
        .find(|n| n.id == incoming.source)
        .unwrap()
        .syntax_unit
        .as_ref()
        .unwrap();
    assert_eq!(
        result.graph.syntax_units[parent].context.as_deref(),
        Some("attributes")
    );
}

#[test]
fn incomplete_host_scopes_cannot_supply_confident_foreign_bindings() {
    let (host, target) = profiles(true);
    let result = analyze(host, target, "VAR host; ~[@(host)]~ VAR broken");
    assert!(!result.complete);
    assert_eq!(references(&result, ResolutionState::Resolved), 0);
    assert_eq!(references(&result, ResolutionState::Unresolved), 1);
}

#[test]
fn foreign_writes_remove_call_certainty_and_foreign_calls_remain_explicitly_pending() {
    let (host, target) = profiles(true);
    let result = analyze(
        host.clone(),
        target.clone(),
        "FUNC run() {} EVAL run(); ~[@(run = 1)]~",
    );
    assert!(result.complete);
    let call = result
        .graph
        .edges
        .iter()
        .find(|e| e.kind == EdgeKind::Calls)
        .unwrap();
    assert_eq!(call.resolution, ResolutionState::Unresolved);
    assert_eq!(
        call.resolution_reason.as_deref(),
        Some("composed_mutation_prevents_call_certainty")
    );
    let result = analyze(host.clone(), target.clone(), "FUNC run() {} ~[@(run())]~");
    assert_eq!(references(&result, ResolutionState::Resolved), 1);
    let call = result
        .graph
        .edges
        .iter()
        .find(|e| e.kind == EdgeKind::Calls)
        .unwrap();
    assert_eq!(call.resolution, ResolutionState::Unresolved);
    assert_eq!(
        call.resolution_reason.as_deref(),
        Some("composed_call_resolution_pending")
    );
    let result = analyze(host, target, "FUNC run() {} EVAL run(); ~[@(missing = 1)]~");
    assert!(
        !result
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Calls && e.resolution == ResolutionState::Resolved)
    );
}

#[test]
fn embedded_declarations_do_not_leak_to_hosts_or_sibling_interpretations() {
    let (mut host, mut target) = profiles(true);
    for profile in [&mut host, &mut target] {
        profile
            .manifest
            .composition
            .as_mut()
            .unwrap()
            .routes
            .get_mut("code")
            .unwrap()
            .context = "body".into();
    }
    let result = analyze(
        host,
        target,
        "~[@(VAR inside; READ inside;) @(READ inside;)]~ READ inside;",
    );
    assert_eq!(references(&result, ResolutionState::Resolved), 1);
    assert_eq!(references(&result, ResolutionState::Unresolved), 2);
}

#[test]
fn scope_bridges_require_the_new_manifest_and_graph_contracts() {
    let (mut host, _) = profiles(true);
    host.manifest.composition.as_mut().unwrap().schema_version = 3;
    assert!(host.validate().is_err());
    let (mut host, _) = profiles(true);
    host.graph.schema_version = 8;
    assert!(host.validate().is_err());
}

#[test]
fn foreign_binding_proofs_reject_deleted_bridges_wrong_targets_and_changed_scope_policies() {
    let (host, target) = profiles(true);
    let result = analyze(host, target, "VAR host; VAR other; ~[@(host)]~");
    for mutation in [
        "bridge",
        "target",
        "request",
        "policy",
        "proof",
        "scope_cycle",
    ] {
        let mut graph = result.graph.clone();
        match mutation {
            "bridge" => graph.edges.retain(|e| e.kind != EdgeKind::ScopeBridge),
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
                    .find(|e| e.kind == EdgeKind::References)
                    .unwrap()
                    .target = other;
            }
            "request" => {
                graph
                    .edges
                    .iter_mut()
                    .find(|e| e.kind == EdgeKind::References)
                    .unwrap()
                    .scope_binding
                    .as_mut()
                    .unwrap()
                    .name = "other".into()
            }
            "policy" => {
                graph
                    .nodes
                    .iter_mut()
                    .find(|n| n.kind == nexora_graph::NodeKind::SyntaxUnit)
                    .unwrap()
                    .scope_policy
                    .as_mut()
                    .unwrap()
                    .parent_lookup = false
            }
            "scope_cycle" => {
                let node = graph
                    .nodes
                    .iter_mut()
                    .find(|n| n.kind == nexora_graph::NodeKind::File)
                    .unwrap();
                node.scope_id = node.owned_scope_id.clone();
            }
            "proof" => {
                graph
                    .edges
                    .iter_mut()
                    .find(|e| e.kind == EdgeKind::References)
                    .unwrap()
                    .scope_binding = None
            }
            _ => unreachable!(),
        }
        assert!(
            nexora_graph::GraphIntegrityValidator::validate(&graph).is_err(),
            "accepted invalid {mutation}"
        );
    }
}

#[test]
fn composed_bindings_survive_cache_reuse_and_recompute_after_editing_the_host() {
    use nexora_core::TenantId;
    use nexora_indexer::{IndexCache, Indexer, MemoryStore};
    let (host, target) = profiles(true);
    let indexer = Indexer::with_catalog(host, vec![target]).unwrap();
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-scope-cache-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.nxb"), "VAR host; <Box count={host}/>").unwrap();
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let tenant = TenantId("scope-test".into());
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
    std::fs::write(root.join("a.nxb"), "VAR renamed; <Box count={host}/>").unwrap();
    let changed = indexer
        .index_incremental(&root, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(changed.reuse.reused_files, 0);
    let full = indexer
        .index(&root, tenant, &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&changed.snapshot).unwrap(),
        serde_json::to_value(full).unwrap()
    );
    assert!(
        changed
            .snapshot
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::References && e.resolution == ResolutionState::Unresolved)
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_collect_ambiguous_callee_cannot_keep_a_locally_resolved_call() {
    let (mut host, mut target) = profiles(true);
    for profile in [&mut host, &mut target] {
        let route = profile
            .manifest
            .composition
            .as_mut()
            .unwrap()
            .routes
            .get_mut("code")
            .unwrap();
        route.context = "body".into();
        route.scope.shadowing = ShadowingPolicy::Collect;
    }
    let result = analyze(
        host,
        target,
        "FUNC run() {} ~[@(FUNC run() {} EVAL run();)]~",
    );
    assert_eq!(references(&result, ResolutionState::Ambiguous), 1);
    assert!(
        !result
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Calls && e.resolution == ResolutionState::Resolved)
    );
}

#[test]
fn changing_both_the_requested_name_and_target_does_not_forge_reference_evidence() {
    let (host, target) = profiles(true);
    let mut graph = analyze(host, target, "VAR host; VAR other; ~[@(host)]~").graph;
    let other = graph
        .nodes
        .iter()
        .find(|n| n.name == "other")
        .unwrap()
        .id
        .clone();
    let edge = graph
        .edges
        .iter_mut()
        .find(|e| e.kind == EdgeKind::References)
        .unwrap();
    edge.target = other;
    edge.scope_binding.as_mut().unwrap().name = "other".into();
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
}

#[test]
fn snapshot_validation_preserves_independent_per_file_lookup_budgets() {
    use nexora_core::TenantId;
    use nexora_indexer::{Indexer, MemoryStore};
    let (host, target) = profiles(true);
    let indexer = Indexer::with_catalog(host, vec![target]).unwrap();
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-scope-budget-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = format!(
        "VAR host; FUNC test() {{ {}{}{} }}",
        "BLOCK {".repeat(30),
        "READ host;".repeat(1100),
        "}".repeat(30)
    );
    for name in ["a.nxb", "b.nxb", "c.nxb"] {
        std::fs::write(root.join(name), &source).unwrap();
    }
    let result = indexer.index(
        &root,
        TenantId("scope-budget-test".into()),
        &mut MemoryStore::default(),
    );
    std::fs::remove_dir_all(root).unwrap();
    let snapshot = result.unwrap();
    assert_eq!(
        snapshot
            .graph
            .nodes
            .iter()
            .filter(|n| n.kind == nexora_graph::NodeKind::File)
            .count(),
        3
    );
    assert_eq!(
        snapshot
            .graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::References && e.resolution == ResolutionState::Resolved)
            .count(),
        3300
    );
}

#[test]
fn a_reusable_scoped_catalog_resolves_parameters_and_header_expressions() {
    let root = Path::new("../../templates/composition-scopes");
    let host = LanguageDefinition::load(&root.join("region_carrier")).unwrap();
    let target = LanguageDefinition::load(&root.join("region_expression")).unwrap();
    let source = std::fs::read_to_string(root.join("fixtures/complete/source.nxb")).unwrap();
    let result = analyze(host, target, &source);
    assert!(result.complete);
    assert_eq!(references(&result, ResolutionState::Resolved), 4);
    assert_eq!(references(&result, ResolutionState::Unresolved), 0);
    assert!(result.units.iter().all(|u| u.diagnostics.is_empty()));
}

#[test]
fn nearest_shadowing_keeps_the_embedded_declaration_and_a_disabled_header_stops_lookup() {
    let (mut host, mut target) = profiles(true);
    for profile in [&mut host, &mut target] {
        profile
            .manifest
            .composition
            .as_mut()
            .unwrap()
            .routes
            .get_mut("code")
            .unwrap()
            .context = "body".into();
    }
    let result = analyze(host, target, "VAR name; ~[@(VAR name; READ name;)]~");
    assert_eq!(references(&result, ResolutionState::Resolved), 1);
    let edge = result
        .graph
        .edges
        .iter()
        .find(|e| e.kind == EdgeKind::References)
        .unwrap();
    assert_eq!(
        result
            .graph
            .nodes
            .iter()
            .find(|n| n.id == edge.target)
            .unwrap()
            .syntax_unit,
        edge.evidence.syntax_unit
    );
    let (mut host, mut target) = profiles(true);
    for profile in [&mut host, &mut target] {
        profile
            .manifest
            .composition
            .as_mut()
            .unwrap()
            .header_routes
            .get_mut("element")
            .unwrap()
            .scope
            .parent_lookup = false;
    }
    let result = analyze(host, target, "VAR host; <Box count={host}/>");
    assert_eq!(references(&result, ResolutionState::Unresolved), 1);
    assert_eq!(references(&result, ResolutionState::Resolved), 0);
}

#[test]
fn explicitly_configured_lexical_exports_follow_the_same_binding_decision() {
    let (mut host, mut target) = profiles(true);
    for profile in [&mut host, &mut target] {
        let route = profile
            .manifest
            .composition
            .as_mut()
            .unwrap()
            .routes
            .get_mut("code")
            .unwrap();
        route.context = "body".into();
        route.scope.shadowing = ShadowingPolicy::Collect;
        profile
            .graph
            .reference_relations
            .insert("read".into(), vec!["REFERENCES".into(), "EXPORTS".into()]);
    }
    let result = analyze(
        host.clone(),
        target.clone(),
        "VAR name; ~[@(VAR name; READ name;)]~",
    );
    assert_eq!(references(&result, ResolutionState::Ambiguous), 1);
    assert!(
        result
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Exports && e.resolution == ResolutionState::Ambiguous)
    );
    let result = analyze(host, target, "VAR host; ~[@(READ host;)]~");
    assert_eq!(references(&result, ResolutionState::Resolved), 1);
    assert!(
        result
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Exports && e.resolution == ResolutionState::Resolved)
    );
}
