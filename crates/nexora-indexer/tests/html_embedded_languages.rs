use nexora_core::RegionStatus;
use nexora_graph::{EdgeKind, NodeKind};
use nexora_indexer::{CompositionAnalyzer, CompositionResult};
use nexora_language::LanguageDefinition;
use std::path::Path;

fn analyze(source: &str) -> Result<CompositionResult, nexora_core::NexoraError> {
    let root = Path::new("../../templates/web-documents");
    let html = LanguageDefinition::load(&root.join("html"))?;
    CompositionAnalyzer::new(
        html,
        vec![
            LanguageDefinition::load(&root.join("javascript"))?,
            LanguageDefinition::load(&root.join("css"))?,
        ],
    )?
    .analyze_source("source.html", source)
}

#[test]
fn real_html_produces_distinct_javascript_css_and_nested_expression_units() {
    let js = "const base = 2; function render(value) { return `${value + base}`; }";
    let css = ".panel { color: red; }";
    let source = format!(
        "<main>literal function forged() {{}}</main><script>{js}</script><style>{css}</style>"
    );
    let result = analyze(&source).unwrap();
    let languages: Vec<_> = result.units.iter().map(|u| u.language.0.as_str()).collect();
    assert_eq!(languages, ["html", "javascript", "css", "javascript"]);
    for (language, context, text) in [
        ("javascript", "script", js),
        ("css", "stylesheet", css),
        ("javascript", "expression", "value + base"),
    ] {
        let unit = result
            .units
            .iter()
            .find(|u| u.language.0 == language && u.context.as_deref() == Some(context))
            .unwrap();
        assert_eq!(&source[unit.span.start.offset..unit.span.end.offset], text);
    }
    assert!(
        result
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Function && n.name == "render")
    );
    assert!(
        !result
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Function && n.name == "forged")
    );
    assert!(
        result
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Interprets)
    );
    let references: Vec<_> = result
        .graph
        .edges
        .iter()
        .filter(|e| {
            e.kind == EdgeKind::References
                && e.resolution == nexora_graph::ResolutionState::Resolved
        })
        .collect();
    assert_eq!(references.len(), 2);
    for reference in references {
        let target = result
            .graph
            .nodes
            .iter()
            .find(|n| n.id == reference.target)
            .unwrap();
        assert!(matches!(target.name.as_str(), "value" | "base"));
        let source_unit = reference
            .evidence
            .syntax_unit
            .as_ref()
            .and_then(|id| result.graph.syntax_units.get(id))
            .unwrap();
        let target_unit = target
            .syntax_unit
            .as_ref()
            .and_then(|id| result.graph.syntax_units.get(id))
            .unwrap();
        assert_eq!(source_unit.context.as_deref(), Some("expression"));
        assert_eq!(target_unit.context.as_deref(), Some("script"));
    }

    assert!(
        !result.complete,
        "HTML fragments and CSS rule interiors stay unknown"
    );
}

#[test]
fn data_external_and_unknown_script_headers_never_become_javascript_declarations() {
    for header in [
        "type='application/ld+json'",
        "type='text/typescript'",
        "src='external.js'",
        "language='vbscript'",
        "type='module' type='text/javascript'",
    ] {
        let result = analyze(&format!(
            "<script {header}>function forged() {{}}</script><style>.ok {{color:red}}</style>"
        ))
        .unwrap();
        assert!(
            !result.units.iter().any(|u| u.language.0 == "javascript"),
            "{header}"
        );
        assert!(
            !result
                .graph
                .nodes
                .iter()
                .any(|n| n.kind == NodeKind::Function && n.name == "forged")
        );
        assert!(result.units.iter().any(|u| u.language.0 == "css"));
    }
}

#[test]
fn mixed_case_tags_and_quoted_header_text_preserve_actual_language_boundaries() {
    let source = "<ScRiPt data-note=\"<style> fake >\" TYPE='text/javascript' nonce=abc>const actual = 1;</sCrIpT><STYLE media='screen'>.ok{color:red}</style>";
    let result = analyze(source).unwrap();
    assert_eq!(result.units.len(), 3);
    assert_eq!(result.units[1].status, RegionStatus::Parsed);
    assert!(
        result
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Variable && n.name == "actual")
    );
}

#[test]
fn comments_raw_text_and_quoted_attributes_cannot_create_script_units() {
    for hidden in [
        "<!-- <script>function forged() {}</script> -->",
        "<textarea><script>function forged() {}</script></textarea>",
        "<title><script>function forged() {}</script></title>",
        "<xmp><script>function forged() {}</script></xmp>",
        "<iframe><script>function forged() {}</script></iframe>",
        "<div title=\"<script>function forged() {}</script>\"></div>",
        "<style>.x{content:'<script>function forged() {}</script>';}</style>",
    ] {
        let result = analyze(&format!("{hidden}<style>.ok{{color:red}}</style>")).unwrap();
        assert!(
            !result.units.iter().any(|u| u.language.0 == "javascript"),
            "{hidden}"
        );
        assert!(
            !result
                .graph
                .nodes
                .iter()
                .any(|n| n.kind == NodeKind::Function)
        );
    }
    for source in [
        "<!-- <script>function forged() {}</script>",
        "<div title='<script>function forged() {}</script>",
    ] {
        let result = analyze(source).unwrap();
        assert_eq!(result.units.len(), 1);
        assert!(!result.complete);
    }
}

#[test]
fn host_rawtext_closes_inside_a_foreign_string_and_preserves_neighboring_units() {
    let source = "<script>const text = \"</script>\";<style>.ok{color:red}</style><script>const after = 1;</script>";
    let result = analyze(source).unwrap();
    assert_eq!(result.units.len(), 4);
    assert_eq!(
        &source[result.units[1].span.start.offset..result.units[1].span.end.offset],
        "const text = \""
    );
    assert_eq!(result.units[1].status, RegionStatus::Partial);
    assert!(result.units[1].graph.nodes.is_empty());
    assert!(result.units.iter().any(|u| u.language.0 == "css"));
    assert!(
        result
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Variable && n.name == "after")
    );
    assert!(!result.complete);
}

#[test]
fn end_tag_name_boundaries_ascii_case_and_uncertain_headers_are_explicit() {
    for header in [
        "type=module",
        "type='MODULE'",
        "id=x type='text/javascript' data-src='inline'",
        "data-note=\"src='external' type='application/json'\"",
        "srcc=abc",
        "type='application/javascript' nonce=x",
    ] {
        let result = analyze(&format!(
            "<script {header}>const text = '</scripture>'; const actual = 1;</SCRIPT>"
        ))
        .unwrap();
        assert!(
            result
                .graph
                .nodes
                .iter()
                .any(|n| n.kind == NodeKind::Variable && n.name == "actual"),
            "{header}"
        );
    }
    for header in [
        "SrC='external.js' type=module",
        "type=' module '",
        "TYPE='application/json'",
        "type='text/javascript' type=module",
    ] {
        let result = analyze(&format!("<script {header}>function forged() {{}}</script>")).unwrap();
        assert_eq!(result.units.len(), 1, "{header}");
    }
    let result = analyze("<ſcript>function forged() {}</ſcript><style>.ok{}</style>").unwrap();
    assert!(!result.units.iter().any(|u| u.language.0 == "javascript"));
}

#[test]
fn unsupported_host_states_fail_explicitly_instead_of_guessing_foreign_regions() {
    for source in [
        "<svg><script>function forged() {}</script></svg>",
        "<math><script>function forged() {}</script></math>",
        "<noscript><script>function forged() {}</script></noscript>",
        "<script><!--<script>hidden</script> --></script>",
    ] {
        assert!(analyze(source).is_err(), "{source}");
    }
}

#[test]
fn empty_and_unclosed_foreign_bodies_keep_boundary_evidence() {
    let empty = analyze("<script></script><style></style>").unwrap();
    assert_eq!(empty.units.len(), 3);
    assert!(
        empty
            .units
            .iter()
            .skip(1)
            .all(|u| u.status == RegionStatus::Parsed)
    );
    let partial = analyze("<script>const value = 1;").unwrap();
    assert_eq!(partial.units.len(), 2);
    assert!(!partial.complete);
    assert!(
        partial
            .interpretations
            .iter()
            .all(|link| !link.boundary_closed && link.status == RegionStatus::Partial)
    );
}

#[test]
fn separate_script_units_do_not_invent_shared_globals_or_html_bindings() {
    let result =
        analyze("<script>const hidden = 1;</script><script>const text = `${hidden}`;</script>")
            .unwrap();
    let refs: Vec<_> = result
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::References)
        .collect();
    assert_eq!(refs.len(), 1);
    assert_eq!(
        refs[0].resolution,
        nexora_graph::ResolutionState::Unresolved
    );
}

#[test]
fn malformed_end_tag_openers_do_not_expose_script_text_from_bogus_comments() {
    for prefix in ["</!", "</?", "</é"] {
        let source = format!("{prefix}<script>const forged = 1;</script><style>.ok{{}}</style>");
        let result = analyze(&source).unwrap();
        assert!(
            !result.units.iter().any(|u| u.language.0 == "javascript"),
            "{source}"
        );
        assert!(
            !result
                .graph
                .nodes
                .iter()
                .any(|n| n.kind == NodeKind::Variable && n.name == "forged")
        );
        assert!(result.units.iter().any(|u| u.language.0 == "css"));
    }
}

#[test]
fn html_alternate_and_abrupt_comment_closures_preserve_following_languages() {
    for comment in ["<!-- --!>", "<!-->", "<!--->"] {
        let source = format!("{comment}<script>const actual = 1;</script><style>.ok{{}}</style>");
        let result = analyze(&source).unwrap();
        assert!(
            result
                .graph
                .nodes
                .iter()
                .any(|n| n.kind == NodeKind::Variable && n.name == "actual"),
            "{comment}"
        );
        assert_eq!(result.units.len(), 3);
    }
}

#[test]
fn plaintext_preserves_prior_units_and_masks_all_apparent_closers_until_input_end() {
    let source = "<script>const actual = 1;</script><style>.ok{color:red}</style><PlaInTeXt title='literal'>é </plaintext><script>function forged() {}</script><style>.forged{}</style>";
    let result = analyze(source).unwrap();
    assert_eq!(result.units.len(), 3);
    assert!(
        result
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Variable && n.name == "actual")
    );
    assert!(
        !result
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Function && n.name == "forged")
    );
    let region = result.units[0]
        .graph
        .nodes
        .iter()
        .filter_map(|n| n.lexical_region.as_ref())
        .find(|r| r.mode == "plaintext")
        .unwrap();
    assert!(region.closed && region.closer.is_none());
    assert_eq!(region.content_span.end.offset, source.len());
    assert_eq!(
        &source[region.content_span.start.offset..region.content_span.end.offset],
        "é </plaintext><script>function forged() {}</script><style>.forged{}</style>"
    );
    assert!(!result.complete, "HTML/CSS semantics remain unknown");
}
