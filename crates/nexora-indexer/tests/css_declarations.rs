use nexora_core::RegionStatus;
use nexora_graph::NodeKind;
use nexora_indexer::{CompositionAnalyzer, CompositionResult};
use nexora_language::LanguageDefinition;
use std::path::Path;
fn analyze(source: &str) -> CompositionResult {
    let model = LanguageDefinition::load(Path::new("../../templates/web-documents/css")).unwrap();
    CompositionAnalyzer::new(model, vec![])
        .unwrap()
        .analyze_source("source.css", source)
        .unwrap()
}
fn properties(result: &CompositionResult) -> Vec<&nexora_ast::AbstractSyntaxNode> {
    result.units[0]
        .ast
        .nodes
        .iter()
        .filter(|n| n.kind == "css_declaration" || n.kind == "css_empty_declaration")
        .collect()
}
#[test]
fn declarations_preserve_property_names_and_exact_component_value_spans() {
    let source = ".panel { color: red; width: calc(100% - 2rem); content: \"a;b}\"; margin: 0 }";
    let result = analyze(source);
    let nodes = properties(&result);
    assert_eq!(nodes.len(), 4);
    for (name, value) in [
        ("color", "red"),
        ("width", "calc(100% - 2rem)"),
        ("content", "\"a;b}\""),
        ("margin", "0"),
    ] {
        let node = nodes.iter().find(|n| n.name == name).unwrap();
        let region = node.regions.iter().find(|r| r.capture == "value").unwrap();
        assert_eq!(
            &source[region.span.start.offset..region.span.end.offset],
            value
        );
        assert_eq!(region.status, RegionStatus::Parsed);
    }
    assert!(
        !result.complete,
        "selectors and property semantics remain unknown"
    );
}
#[test]
fn conditional_and_descriptor_rules_preserve_their_actual_declaration_children() {
    let result = analyze(
        "@media (min-width: 600px) { .panel { color: red; @supports (display:grid) { display: grid; } } } @font-face { font-family: Sample; src: url(\"font;a.woff2\"); }",
    );
    let names: Vec<_> = properties(&result)
        .into_iter()
        .map(|n| n.name.as_str())
        .collect();
    assert_eq!(names, ["color", "display", "font-family", "src"]);
}
#[test]
fn custom_property_blocks_strings_and_urls_do_not_create_false_rules_or_lexical_bindings() {
    let result = analyze(
        ".panel { --payload: { nested: \"}\"; count: 1 }; color: var(--accent); background: url(data:image/x;a={b}); }",
    );
    let nodes = properties(&result);
    assert_eq!(nodes.len(), 3);
    assert!(
        !nodes
            .iter()
            .any(|n| n.name == "nested" || n.name == "count")
    );
    assert!(
        !result
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Variable || n.kind == NodeKind::Function)
    );
    assert!(
        result
            .graph
            .edges
            .iter()
            .all(|e| e.kind != nexora_graph::EdgeKind::Calls)
    );
}
#[test]
fn nested_selector_colons_and_trivia_before_parentheses_are_not_invented_properties_or_functions() {
    let result = analyze(".outer { a:hover { color: red; } width: calc /* comment */ (1px); }");
    let names: Vec<_> = properties(&result)
        .into_iter()
        .map(|n| n.name.as_str())
        .collect();
    assert_eq!(names, ["color", "width"]);
    assert!(
        !result.units[0]
            .ast
            .nodes
            .iter()
            .any(|n| n.kind == "css_function" && n.name == "calc")
    );
}
#[test]
fn invalid_declaration_body_stays_partial_without_erasing_a_following_rule() {
    let result = analyze(".bad { broken ???; color: red; } .after { width: 1px; }");
    assert_eq!(result.units[0].status, RegionStatus::Partial);
    let nodes = properties(&result);
    assert!(nodes.iter().any(|n| n.name == "width"));
    assert!(!nodes.iter().any(|n| n.name == "color"));
}

#[test]
fn escaped_url_names_preserve_raw_url_boundaries_and_following_rules() {
    for name in [
        r"u\72l",
        r"\75rl",
        r"\u\r\l",
        r"\000075\000072\00006c",
        r"u\000052l",
    ] {
        let result = analyze(&format!(
            ".x {{ background: {name}(data:x,{{); color:red; }} .after {{ width:1px }}"
        ));
        let names: Vec<_> = properties(&result)
            .into_iter()
            .map(|n| n.name.as_str())
            .collect();
        assert_eq!(names, ["background", "color", "width"], "{name}");
    }
}

#[test]
fn dense_component_values_keep_their_structure_under_bounded_file_work() {
    let source = format!(".dense {{ {} }}", "color:rgb(1,2,3);".repeat(2500));
    let result = analyze(&source);
    assert_eq!(properties(&result).len(), 2500);
    assert_eq!(
        result.units[0]
            .ast
            .nodes
            .iter()
            .filter(|n| n.kind == "css_function" && n.name == "rgb")
            .count(),
        2500
    );
    assert!(!result.complete);
    let model = LanguageDefinition::load(Path::new("../../templates/web-documents/css")).unwrap();
    let limited = CompositionAnalyzer::with_limits(
        model,
        vec![],
        nexora_indexer::CompositionLimits {
            max_nodes: 2000,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        limited.analyze_source("limited.css", &source),
        Err(nexora_core::NexoraError::Limit(_))
    ));
}
