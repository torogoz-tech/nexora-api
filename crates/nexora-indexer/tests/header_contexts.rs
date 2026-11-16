use nexora_core::RegionStatus;
use nexora_indexer::{CompositionAnalyzer, CompositionResult};
use nexora_language::LanguageDefinition;
use std::path::Path;
fn profiles() -> (LanguageDefinition, LanguageDefinition) {
    let root = Path::new("../../templates/composition-markup");
    (
        LanguageDefinition::load(&root.join("region_carrier")).unwrap(),
        LanguageDefinition::load(&root.join("region_expression")).unwrap(),
    )
}

fn analyze(source: &str) -> CompositionResult {
    let (host, target) = profiles();
    CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("header.nxb", source)
        .unwrap()
}

#[test]
fn headers_have_their_own_grammar_and_real_attribute_expression_nodes() {
    let result = analyze("VAR host; <Box count={(1 + 2)} title=\"a > b\">text</Box> READ host;");
    assert!(result.complete);
    assert!(
        result
            .units
            .iter()
            .any(|u| u.context.as_deref() == Some("attributes")
                && u.ast.nodes.iter().any(|n| n.name == "count"))
    );
    assert!(
        result
            .units
            .iter()
            .any(|u| u.tokens.iter().map(|t| t.text.as_str()).collect::<String>() == "(1 + 2)")
    );
    let element = result.units[0]
        .cst
        .nodes
        .iter()
        .find(|n| n.name == "element")
        .unwrap();
    assert_eq!(
        element
            .regions
            .iter()
            .find(|r| r.capture == "embedded_header")
            .unwrap()
            .status,
        RegionStatus::Parsed
    );
    assert!(
        result
            .interpretations
            .iter()
            .any(|l| l.component == nexora_core::LexicalRegionComponent::Header)
    );
}

#[test]
fn self_closed_header_input_excludes_both_delimiters_and_preserves_unicode_positions() {
    let result = analyze("\r\n<Box title=\"café\"/>");
    assert!(result.complete);
    let header = result
        .units
        .iter()
        .find(|u| u.context.as_deref() == Some("attributes"))
        .unwrap();
    assert_eq!(
        &result.source.content[header.span.start.offset..header.span.end.offset],
        " title=\"café\""
    );
    let element = result.units[0]
        .cst
        .nodes
        .iter()
        .find_map(|n| n.lexical_region.as_ref())
        .unwrap();
    assert!(element.self_closed && element.header_closed);
    assert_eq!(element.header_content_span, Some(header.span));
    assert!(header.span.end.offset - header.span.start.offset < result.source.content.len());
}

#[test]
fn a_closed_header_remains_known_when_the_body_is_truncated() {
    let result = analyze("<Box title=\"text\">body");
    assert!(!result.complete);
    let element = result.units[0]
        .cst
        .nodes
        .iter()
        .find(|n| n.name == "element")
        .unwrap();
    assert_eq!(
        element
            .regions
            .iter()
            .find(|r| r.capture == "embedded_header")
            .unwrap()
            .status,
        RegionStatus::Parsed
    );
    assert_eq!(
        element
            .regions
            .iter()
            .find(|r| r.capture == "embedded_content")
            .unwrap()
            .status,
        RegionStatus::Partial
    );
}

#[test]
fn a_valid_prefix_does_not_make_an_unclosed_header_complete() {
    let result = analyze("<Box title=\"text\"");
    assert!(!result.complete);
    let header = result
        .units
        .iter()
        .find(|u| u.context.as_deref() == Some("attributes"))
        .unwrap();
    assert_eq!(header.status, RegionStatus::Partial);
    assert!(
        !result.units[0]
            .cst
            .nodes
            .iter()
            .find_map(|n| n.lexical_region.as_ref())
            .unwrap()
            .header_closed
    );
}

#[test]
fn invalid_header_grammar_does_not_hide_a_known_body() {
    let result = analyze("<Box unknown>body</Box>");
    assert!(!result.complete);
    let element = result.units[0]
        .cst
        .nodes
        .iter()
        .find(|n| n.name == "element")
        .unwrap();
    assert_eq!(
        element
            .regions
            .iter()
            .find(|r| r.capture == "embedded_content")
            .unwrap()
            .status,
        RegionStatus::Parsed
    );
    assert_eq!(
        element
            .regions
            .iter()
            .find(|r| r.capture == "embedded_header")
            .unwrap()
            .status,
        RegionStatus::Partial
    );
}

#[test]
fn a_partial_empty_body_does_not_invalidate_the_independent_self_closed_header() {
    let (mut host, target) = profiles();
    let mut manifest = serde_json::to_value(&host.manifest).unwrap();
    manifest["composition"]["roles"]["element"] = "grammar".into();
    manifest["composition"]["routes"]["element"] =
        manifest["composition"]["routes"]["code"].clone();
    host.manifest = serde_json::from_value(manifest).unwrap();
    let result = CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("header.nxb", "<Box title=\"text\"/>")
        .unwrap();
    let element = result.units[0]
        .cst
        .nodes
        .iter()
        .find(|n| n.name == "element")
        .unwrap();
    assert_eq!(
        element
            .regions
            .iter()
            .find(|r| r.capture == "embedded_content")
            .unwrap()
            .status,
        RegionStatus::Partial
    );
    assert_eq!(
        element
            .regions
            .iter()
            .find(|r| r.capture == "embedded_header")
            .unwrap()
            .status,
        RegionStatus::Parsed
    );
}

#[test]
fn header_interpretations_and_proofs_survive_warm_cache_and_incremental_edits() {
    use nexora_core::TenantId;
    use nexora_indexer::{IndexCache, Indexer, MemoryStore};
    let (host, target) = profiles();
    let indexer = Indexer::with_catalog(host, vec![target]).unwrap();
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "nexora-header-interpretation-cache-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.nxb"), "<Box count={(1 + 2)} title=\"café\"/>").unwrap();
    std::fs::write(root.join("b.nxb"), "<Other title=\"stable\">text</Other>").unwrap();
    let tenant = TenantId("header-proof-test".into());
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let cold = indexer
        .index_incremental(&root, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    let warm = indexer
        .index_incremental(&root, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 2);
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    assert!(warm.snapshot.graph.nodes.iter().any(|n| {
        n.regions
            .iter()
            .any(|r| r.capture == "embedded_header" && r.coverage.is_some())
    }));
    std::fs::write(
        root.join("a.nxb"),
        "<Box count={(3 * 4)} title=\"changed\"><Leaf title=\"nested\"/></Box>",
    )
    .unwrap();
    let changed = indexer
        .index_incremental(&root, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(changed.reuse.reused_files, 1);
    let full = indexer
        .index(&root, tenant, &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&changed.snapshot).unwrap(),
        serde_json::to_value(&full).unwrap()
    );
    nexora_graph::GraphIntegrityValidator::validate(&full.graph).unwrap();
    assert!(
        full.graph
            .edges
            .iter()
            .any(|e| e.kind == nexora_graph::EdgeKind::Interprets
                && e.label.as_deref() == Some("header"))
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn header_proofs_reject_missing_evidence_wrong_components_and_corrupt_ranges() {
    use nexora_graph::{EdgeKind, GraphIntegrityValidator};
    let result = analyze("<Box count={(1 + 2)} title=\"café\"/>");
    GraphIntegrityValidator::validate(&result.graph).unwrap();
    let element_id = result
        .graph
        .nodes
        .iter()
        .find(|n| n.name == "element")
        .unwrap()
        .id
        .clone();
    for mutation in [
        "erase_header",
        "erase_proof",
        "wrong_unit",
        "wrong_label",
        "unclosed_header",
        "wrong_input",
        "wrong_header",
    ] {
        let mut graph = result.graph.clone();
        let node = graph.nodes.iter_mut().find(|n| n.id == element_id).unwrap();
        match mutation {
            "erase_header" => node.regions.retain(|r| r.capture != "embedded_header"),
            "erase_proof" => {
                node.regions
                    .iter_mut()
                    .find(|r| r.capture == "embedded_header")
                    .unwrap()
                    .coverage = None
            }
            "wrong_unit" => {
                node.regions
                    .iter_mut()
                    .find(|r| r.capture == "embedded_header")
                    .unwrap()
                    .coverage
                    .as_mut()
                    .unwrap()
                    .syntax_units[0] = node.syntax_unit.clone().unwrap()
            }
            "wrong_label" => {
                graph
                    .edges
                    .iter_mut()
                    .find(|e| {
                        e.kind == EdgeKind::Interprets
                            && e.source == element_id
                            && e.label.as_deref() == Some("header")
                    })
                    .unwrap()
                    .label = Some("content".into())
            }
            "unclosed_header" => node.lexical_region.as_mut().unwrap().header_closed = false,
            "wrong_input" => {
                node.lexical_region
                    .as_mut()
                    .unwrap()
                    .header_content_span
                    .as_mut()
                    .unwrap()
                    .end
                    .offset -= 1
            }
            "wrong_header" => node.lexical_region.as_mut().unwrap().header = None,
            _ => unreachable!(),
        }
        assert!(
            GraphIntegrityValidator::validate(&graph).is_err(),
            "accepted corrupt header evidence: {mutation}"
        );
    }
}

#[test]
fn reusable_markup_catalog_analyzes_real_attribute_grammar_and_nested_interpretations() {
    let root = Path::new("../../templates/composition-markup");
    let host = LanguageDefinition::load(&root.join("region_carrier")).unwrap();
    let target = LanguageDefinition::load(&root.join("region_expression")).unwrap();
    let source = std::fs::read_to_string(root.join("fixtures/complete/source.nxb")).unwrap();
    let result = CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("markup.nxb", &source)
        .unwrap();
    assert!(result.complete);
    assert!(result.units.iter().all(|u| u.diagnostics.is_empty()));
    assert!(
        result
            .units
            .iter()
            .any(|u| u.context.as_deref() == Some("attributes"))
    );
    assert!(result.graph.nodes.iter().any(|n| n.name == "count"));
    nexora_graph::GraphIntegrityValidator::validate(&result.graph).unwrap();
}

#[test]
fn a_header_grammar_can_prove_a_direct_guard_child_without_a_body_interpretation() {
    let (mut host, mut target) = profiles();
    for profile in [&mut host, &mut target] {
        profile.manifest.composition.as_mut().unwrap().roles.insert(
            "quote".into(),
            serde_json::from_value(serde_json::json!("guard")).unwrap(),
        );
    }
    let result = CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("header-guard.nxb", "<Box title=\"text\"/>")
        .unwrap();
    assert!(result.complete);
    let quote = result
        .graph
        .nodes
        .iter()
        .find(|n| n.name == "quote")
        .unwrap();
    let proof = quote
        .regions
        .iter()
        .find(|r| r.capture == "embedded_content")
        .unwrap()
        .coverage
        .as_ref()
        .unwrap();
    assert_eq!(proof.method, nexora_core::RegionCoverageMethod::Guard);
    assert_eq!(
        result.graph.syntax_units[&proof.syntax_units[0]]
            .context
            .as_deref(),
        Some("attributes")
    );
    nexora_graph::GraphIntegrityValidator::validate(&result.graph).unwrap();
}

#[test]
fn attribute_expression_tokens_do_not_intercept_function_or_block_braces() {
    let result =
        analyze("FUNC f(p) { READ p; BLOCK { VAR local; READ local; } } <Box count={(1 + 2)}/>");
    assert!(result.complete, "{:?}", result.units[0].diagnostics);
    assert!(
        result
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == nexora_graph::NodeKind::Function && n.name == "f")
    );
    assert!(result.graph.nodes.iter().any(|n| n.name == "local"));
    assert!(result.graph.nodes.iter().any(|n| n.name == "count"));
    assert_eq!(
        result
            .graph
            .edges
            .iter()
            .filter(|e| e.kind == nexora_graph::EdgeKind::References
                && e.resolution == nexora_graph::ResolutionState::Resolved)
            .count(),
        2
    );
}
