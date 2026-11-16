use nexora_core::RegionStatus;
use nexora_graph::{EdgeKind, ResolutionState};
use nexora_indexer::{CompositionAnalyzer, CompositionResult};
use nexora_language::LanguageDefinition;
use std::path::Path;

fn profiles() -> (LanguageDefinition, LanguageDefinition) {
    let root = Path::new("../../templates/composition-molds");
    let mut host = LanguageDefinition::load(&root.join("region_carrier")).unwrap();
    let mut target = LanguageDefinition::load(&root.join("region_expression")).unwrap();
    for profile in [&mut host, &mut target] {
        let mut manifest = serde_json::to_value(&profile.manifest).unwrap();
        manifest["composition"]["schema_version"] = 2.into();
        manifest["composition"]["roles"] = serde_json::json!({"template":"carrier","code":"grammar","paren":"guard","quote":"literal"});
        profile.manifest = serde_json::from_value(manifest).unwrap();
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
        .analyze_source("coverage.nxb", source)
        .unwrap()
}

#[test]
fn literal_carrier_grammar_and_guard_have_auditable_coverage_without_opaque_gaps() {
    let (host, target) = profiles();
    let result = analyze(
        host,
        target,
        "VAR host;\r\n~[café @((1 + 2)) @(\"té\")]~ READ host;",
    );
    assert!(result.complete);
    assert!(
        result
            .units
            .iter()
            .all(|u| u.status == RegionStatus::Parsed)
    );
    assert_eq!(
        result.graph.syntax_coverage["coverage.nxb"].opaque_regions,
        0
    );
    assert_eq!(
        result.graph.syntax_coverage["coverage.nxb"].partial_regions,
        0
    );
    assert!(
        result
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::References && e.resolution == ResolutionState::Resolved)
    );
    let mut methods = std::collections::BTreeSet::new();
    for node in &result.graph.nodes {
        if node.lexical_region.is_some() {
            let region = &node.regions[0];
            assert_eq!(region.status, RegionStatus::Parsed);
            let proof = region
                .coverage
                .as_ref()
                .expect("parsed lexical region needs evidence");
            methods.insert(format!("{:?}", proof.method));
            for unit in &proof.syntax_units {
                assert_eq!(result.graph.syntax_units[unit].status, RegionStatus::Parsed);
            }
            for child in &proof.regions {
                assert!(
                    result
                        .graph
                        .nodes
                        .iter()
                        .any(|n| &n.id == child && n.syntax_unit == node.syntax_unit)
                );
            }
        }
    }
    for method in ["Literal", "Carrier", "Grammar", "Guard"] {
        assert!(methods.contains(method));
    }
}

#[test]
fn unknown_children_are_not_promoted_by_a_successful_ancestor_interpretation() {
    let (mut host, target) = profiles();
    host.manifest
        .composition
        .as_mut()
        .unwrap()
        .roles
        .remove("quote");
    let result = analyze(host, target, "~[@(\"text\")]~");
    assert!(!result.complete);
    let quote = result.units[0]
        .cst
        .nodes
        .iter()
        .find(|n| n.name == "quote")
        .unwrap();
    assert_eq!(quote.regions[0].status, RegionStatus::Opaque);
    assert!(quote.regions[0].coverage.is_none());
    assert_eq!(result.units[1].status, RegionStatus::Parsed);
}

#[test]
fn an_explicit_failing_child_cannot_be_hidden_by_an_outer_string_literal() {
    let (mut host, target) = profiles();
    host.lexer.regions.get_mut("quote").unwrap().children.push(
        serde_json::from_value(serde_json::json!({"pattern":"@\\(","region":"code"})).unwrap(),
    );
    host.manifest.composition.as_mut().unwrap().roles.insert(
        "quote".into(),
        serde_json::from_value(serde_json::json!("carrier")).unwrap(),
    );
    let result = analyze(host, target, "~[@(\"start @(x @ y) end\")]~");
    assert!(!result.complete);
    assert_eq!(result.units[0].status, RegionStatus::Partial);
    assert!(!result.units[0].ast.complete);
    assert!(
        result
            .units
            .iter()
            .any(|u| u.status == RegionStatus::Parsed)
    );
    assert!(
        result
            .units
            .iter()
            .any(|u| u.diagnostics.iter().any(|d| d.stage == "lexer"))
    );
}

#[test]
fn guard_in_a_header_is_not_covered_by_the_body_grammar() {
    let (mut host, target) = profiles();
    let children = host.lexer.regions["code"].children.clone();
    host.lexer.regions.get_mut("code").unwrap().header =
        Some(serde_json::from_value(serde_json::json!({"end":":","children":children})).unwrap());
    let result = analyze(host, target, "~[@((2): 1)]~");
    assert!(!result.complete);
    assert_eq!(result.units[1].status, RegionStatus::Parsed);
    let guard = result.units[0]
        .cst
        .nodes
        .iter()
        .find(|n| n.name == "paren")
        .unwrap();
    assert_eq!(guard.regions[0].status, RegionStatus::Opaque);
}

#[test]
fn opaque_production_captures_are_not_promoted_by_an_equal_literal_span() {
    let (mut host, target) = profiles();
    host.grammar
        .productions
        .iter_mut()
        .find(|p| p.name == "initialized_variable")
        .unwrap()
        .projection
        .regions[0]
        .context = None;
    let result = analyze(host, target, "VAR value = ~[text @(1 + 2)]~;");
    assert!(!result.complete);
    let declaration = result.units[0]
        .cst
        .nodes
        .iter()
        .find(|n| n.name == "value")
        .unwrap();
    assert_eq!(declaration.regions[0].status, RegionStatus::Opaque);
    assert!(declaration.regions[0].coverage.is_none());
}

#[test]
fn nested_reused_units_reach_complete_coverage_with_an_alternate_mold() {
    let (mut host, mut target) = profiles();
    for profile in [&mut host, &mut target] {
        profile
            .lexer
            .tokens
            .iter_mut()
            .find(|t| t.kind == "template")
            .unwrap()
            .pattern = "#\\[".into();
        profile.lexer.regions.get_mut("template").unwrap().close = "]#".into();
        profile.lexer.regions.get_mut("template").unwrap().children[0].pattern = "@<".into();
        profile.lexer.regions.get_mut("code").unwrap().close = ">".into();
        for mode in ["code", "paren"] {
            profile.lexer.regions.get_mut(mode).unwrap().children[1].pattern = "#\\[".into();
        }
    }
    let result = analyze(host, target, "#[outer @<1 + #[inner @<(2 + 3)>]#>]#");
    assert!(result.complete);
    assert_eq!(result.units.len(), 3);
    assert_eq!(result.interpretations.len(), 3);
    assert!(
        result
            .interpretations
            .iter()
            .all(|link| link.status == RegionStatus::Parsed)
    );
}

#[test]
fn an_unclosed_origin_does_not_confirm_calls_from_a_valid_prefix() {
    let (mut host, target) = profiles();
    host.manifest
        .composition
        .as_mut()
        .unwrap()
        .routes
        .get_mut("code")
        .unwrap()
        .context = "body".into();
    let result = analyze(host, target, "~[@(FUNC f() {} EVAL f();");
    assert_eq!(result.units[1].status, RegionStatus::Partial);
    assert!(!result.units[1].ast.complete);
    assert!(
        !result
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Calls && e.resolution == ResolutionState::Resolved)
    );
    assert!(!result.complete);
}

#[test]
fn invalid_roles_and_literal_children_are_rejected_by_configuration() {
    let (mut host, _) = profiles();
    let code = host.manifest.composition.as_mut().unwrap().roles["code"];
    host.manifest
        .composition
        .as_mut()
        .unwrap()
        .roles
        .insert("missing".into(), code);
    assert!(host.validate().is_err());
    let (mut host, _) = profiles();
    host.lexer.regions.get_mut("quote").unwrap().children.push(
        serde_json::from_value(serde_json::json!({"pattern":"@\\(","region":"code"})).unwrap(),
    );
    assert!(host.validate().is_err());
    let (mut host, _) = profiles();
    host.manifest.composition.as_mut().unwrap().schema_version = 1;
    assert!(host.validate().is_err());
}

#[test]
fn coverage_proofs_cannot_be_erased_or_point_to_unrelated_units_or_children() {
    let (host, target) = profiles();
    let result = analyze(host, target, "~[text @((1 + 2)) @(\"text\")]~");
    let mut graph = result.graph.clone();
    graph
        .nodes
        .iter_mut()
        .filter(|node| node.lexical_region.is_some())
        .for_each(|node| node.regions.clear());
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
    let mut graph = result.graph.clone();
    graph
        .nodes
        .iter_mut()
        .find(|n| n.name == "template")
        .unwrap()
        .regions[0]
        .coverage = None;
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
    let mut graph = result.graph.clone();
    graph
        .nodes
        .iter_mut()
        .find(|n| n.name == "template")
        .unwrap()
        .regions[0]
        .coverage
        .as_mut()
        .unwrap()
        .regions
        .clear();
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
    let mut graph = result.graph.clone();
    let owner = graph
        .nodes
        .iter()
        .find(|n| n.name == "code")
        .unwrap()
        .syntax_unit
        .clone()
        .unwrap();
    graph
        .nodes
        .iter_mut()
        .find(|n| n.name == "code")
        .unwrap()
        .regions[0]
        .coverage
        .as_mut()
        .unwrap()
        .syntax_units[0] = owner;
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
    let mut graph = result.graph;
    graph.edges.retain(|edge| edge.kind != EdgeKind::Interprets);
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
}

#[test]
fn partial_interpolation_propagates_to_an_enclosing_parsed_capture() {
    let (host, target) = profiles();
    let result = analyze(host, target, "VAR item = ~[@(x @ y)]~;");
    let declaration = result.units[0]
        .cst
        .nodes
        .iter()
        .find(|n| n.name == "item")
        .unwrap();
    assert_eq!(declaration.regions[0].status, RegionStatus::Partial);
    assert!(!result.units[0].ast.complete);
    assert!(!result.complete);
}

#[test]
fn reusable_schema2_molds_have_complete_coverage_and_real_proofs() {
    let root = Path::new("../../templates/composition-coverage");
    let host = LanguageDefinition::load(&root.join("region_carrier")).unwrap();
    let target = LanguageDefinition::load(&root.join("region_expression")).unwrap();
    let source = std::fs::read_to_string(root.join("fixtures/complete/source.nxb")).unwrap();
    let result = analyze(host, target, &source);
    assert!(result.complete);
    assert!(result.units.iter().all(|u| u.diagnostics.is_empty()));
    assert!(
        result
            .graph
            .nodes
            .iter()
            .filter(|n| n.lexical_region.is_some())
            .all(|n| n.regions[0].coverage.is_some())
    );
    if let Ok(path) = std::env::var("NEXORA_COVERAGE_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    }
}

#[test]
fn coverage_proofs_survive_cache_reuse_and_interior_edits() {
    use nexora_core::TenantId;
    use nexora_indexer::{IndexCache, Indexer, MemoryStore};
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-coverage-cache-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for name in ["a.nxb", "b.nxb"] {
        std::fs::write(root.join(name), "~[@((1 + 2))]~").unwrap();
    }
    let (host, target) = profiles();
    let indexer = Indexer::with_catalog(host, vec![target]).unwrap();
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let cold = indexer
        .index_incremental(&root, TenantId("local".into()), &mut store, &mut cache)
        .unwrap();
    let warm = indexer
        .index_incremental(&root, TenantId("local".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 2);
    assert_eq!(cold.snapshot.id, warm.snapshot.id);
    assert_eq!(
        serde_json::to_value(&cold.snapshot.graph).unwrap(),
        serde_json::to_value(&warm.snapshot.graph).unwrap()
    );
    std::fs::write(root.join("a.nxb"), "~[@((3 + 4))]~").unwrap();
    let edited = indexer
        .index_incremental(&root, TenantId("local".into()), &mut store, &mut cache)
        .unwrap();
    assert_eq!(edited.reuse.reused_files, 1);
    assert_eq!(edited.reuse.analyzed_files, 1);
    let full = indexer
        .index(&root, TenantId("local".into()), &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&edited.snapshot).unwrap(),
        serde_json::to_value(full).unwrap()
    );
    assert!(
        edited
            .snapshot
            .graph
            .nodes
            .iter()
            .filter(|n| n.lexical_region.is_some())
            .all(|n| n.regions[0].coverage.is_some())
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn literal_only_composition_needs_no_grammar_route_and_creates_no_fake_symbols() {
    let (mut host, _) = profiles();
    host.lexer
        .regions
        .get_mut("template")
        .unwrap()
        .children
        .clear();
    let composition = host.manifest.composition.as_mut().unwrap();
    composition.routes.clear();
    composition.roles.clear();
    composition.roles.insert(
        "template".into(),
        nexora_core::RegionCoverageMethod::Literal,
    );
    let result = CompositionAnalyzer::new(host, vec![])
        .unwrap()
        .analyze_source("literal.nxb", "~[VAR fake; FUNC ghost() {}]~")
        .unwrap();
    assert!(result.complete);
    assert!(result.units[0].symbols.symbols.is_empty());
    assert!(result.interpretations.is_empty());
}

#[test]
fn legacy_routes_also_reject_semantic_certainty_for_an_unclosed_origin() {
    let (mut host, mut target) = profiles();
    for profile in [&mut host, &mut target] {
        let c = profile.manifest.composition.as_mut().unwrap();
        c.schema_version = 1;
        c.roles.clear();
    }
    host.manifest
        .composition
        .as_mut()
        .unwrap()
        .routes
        .get_mut("code")
        .unwrap()
        .context = "body".into();
    let result = analyze(host, target, "~[@(FUNC f() {} EVAL f();");
    assert_eq!(result.units[1].status, RegionStatus::Partial);
    assert!(
        !result
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Calls && e.resolution == ResolutionState::Resolved)
    );
}

#[test]
fn header_text_is_not_implicitly_covered_by_a_successful_body_interpretation() {
    let (mut host, target) = profiles();
    host.lexer.regions.get_mut("code").unwrap().header =
        Some(serde_json::from_value(serde_json::json!({"end":":"})).unwrap());
    let result = analyze(host, target, "~[@(VAR hidden: 1)]~");
    assert!(!result.complete);
    let code = result.units[0]
        .cst
        .nodes
        .iter()
        .find(|n| n.name == "code")
        .unwrap();
    let header = code
        .regions
        .iter()
        .find(|r| r.capture == "embedded_header")
        .unwrap();
    assert_eq!(header.status, RegionStatus::Opaque);
    assert!(header.coverage.is_none());
    let mut graph = result.graph;
    graph
        .nodes
        .iter_mut()
        .find(|n| n.name == "code")
        .unwrap()
        .regions
        .retain(|r| r.capture != "embedded_header");
    assert!(nexora_graph::GraphIntegrityValidator::validate(&graph).is_err());
}
