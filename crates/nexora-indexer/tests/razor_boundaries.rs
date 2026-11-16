use nexora_indexer::{CompositionAnalyzer, CompositionResult};
use nexora_language::LanguageDefinition;
use std::path::Path;
fn analyze(source: &str) -> CompositionResult {
    let root = Path::new("../../templates/dotnet-understanding");
    CompositionAnalyzer::new(
        LanguageDefinition::load(&root.join("razor")).unwrap(),
        vec![LanguageDefinition::load(&root.join("csharp")).unwrap()],
    )
    .unwrap()
    .analyze_source("source.razor", source)
    .unwrap()
}
fn contents<'a, 'b>(source: &'a str, result: &'b CompositionResult) -> Vec<(&'b str, &'a str)> {
    result
        .units
        .iter()
        .skip(1)
        .map(|u| {
            (
                u.context.as_deref().unwrap(),
                &source[u.span.start.offset..u.span.end.offset],
            )
        })
        .collect()
}
#[test]
fn explicit_code_member_and_expression_ranges_preserve_nested_csharp_delimiters() {
    let source = r#"<p>@(F(")", new[] { 1, 2 }))</p>@code { string value=@"}""{"; }@{ var x="}"; }<p>@(After())</p>"#;
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("expression_opaque", r#"F(")", new[] { 1, 2 })"#),
            ("members_opaque", r#" string value=@"}""{"; "#),
            ("body_opaque", r#" var x="}"; "#),
            ("expression_opaque", "After()")
        ]
    );
    assert!(!result.complete);
    assert!(
        result
            .graph
            .edges
            .iter()
            .all(|e| e.kind != nexora_graph::EdgeKind::Calls)
    );
}
#[test]
fn quoted_attributes_allow_nested_expression_quotes_without_creating_fake_outer_code() {
    let source = r#"<Panel Title="@(F("quoted > @code { class Fake {} }"))" Other='@(G("x"))' /><p>@(After())</p>"#;
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            (
                "expression_opaque",
                r#"F("quoted > @code { class Fake {} }")"#
            ),
            ("expression_opaque", r#"G("x")"#),
            ("expression_opaque", "After()")
        ]
    );
    assert!(
        result
            .graph
            .nodes
            .iter()
            .all(|n| n.kind != nexora_graph::NodeKind::Class)
    );
}
#[test]
fn comments_and_at_escapes_do_not_fabricate_code_and_unresolved_tails_are_unknown() {
    let source = r#"@* @code { class Fake {} } *@ @@code { class Fake {} } <!-- @(Inside()) --> @(Before()) @try { <p>@value</p> } @code { class UnknownAfter {} }"#;
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("expression_opaque", "Inside()"),
            ("expression_opaque", "Before()")
        ]
    );
    assert!(!result.complete);
    assert!(
        result
            .graph
            .nodes
            .iter()
            .all(|n| n.kind != nexora_graph::NodeKind::Class)
    );
}
#[test]
fn raw_script_and_style_keep_known_razor_expressions_separate_without_claiming_javascript_or_css() {
    let source = r#"<script>const x="@(Value(")"))";</script><style>.x { width:@(Width())px; }</style>@(After())"#;
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("expression_opaque", r#"Value(")")"#),
            ("expression_opaque", "Width()"),
            ("expression_opaque", "After()")
        ]
    );
    assert!(
        result
            .units
            .iter()
            .all(|u| ["razor", "csharp"].contains(&u.language.0.as_str()))
    );
    assert!(!result.complete);
}
#[test]
fn truncated_regions_do_not_receive_complete_child_interpretations() {
    let source = r#"@(F("unfinished")"#;
    let result = analyze(source);
    assert!(!result.complete);
    assert!(
        result
            .units
            .iter()
            .skip(1)
            .all(|u| u.status == nexora_core::RegionStatus::Partial)
    );
    assert!(
        result
            .interpretations
            .iter()
            .all(|i| !i.boundary_closed && i.status == nexora_core::RegionStatus::Partial)
    );
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .any(|r| !r.closed)
    );
}

#[test]
fn razor_comments_inside_code_cannot_close_the_block_or_fabricate_member_routes() {
    let source = r#"@{ var x = @* } @code { Fake } *@ 1; } @(After())"#;
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("body_opaque", " var x = @* } @code { Fake } *@ 1; "),
            ("expression_opaque", "After()")
        ]
    );
    assert!(result.interpretations.iter().all(|i| i.boundary_closed));
    assert!(!result.complete);
}
#[test]
fn doubled_at_in_raw_element_headers_preserves_real_expressions_and_siblings() {
    for tag in ["script", "style"] {
        let source =
            format!("<{tag} data-x=@@foo data-y=@(Header())>@(Inside())</{tag}>@(After())");
        let result = analyze(&source);
        assert_eq!(
            contents(&source, &result),
            [
                ("expression_opaque", "Header()"),
                ("expression_opaque", "Inside()"),
                ("expression_opaque", "After()")
            ]
        );
    }
}

#[test]
fn implicit_expressions_end_before_parent_markup_punctuation_and_adjacent_text() {
    let source = r#"<p>@Name. @Factory(")")[0]?.Title!.</p><Panel Value="@Items[Find("\"")].Name" />@Factory()suffix @(After()) @code { string value="safe"; }"#;
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("expression_opaque", "Name"),
            ("expression_opaque", r#"Factory(")")[0]?.Title"#),
            ("expression_opaque", r#"Items[Find("\"")].Name"#),
            ("expression_opaque", "Factory()"),
            ("expression_opaque", "After()"),
            ("members_opaque", r#" string value="safe"; "#)
        ]
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
fn email_text_and_directive_attribute_names_do_not_become_csharp_expressions() {
    let source = r#"Contact user@example.org <Panel title="é@dominio" @bind-Value="@value" @onclick:preventDefault="true" /> @Name @(After())"#;
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("expression_opaque", "value"),
            ("expression_opaque", "Name"),
            ("expression_opaque", "After()")
        ]
    );
    assert!(!result.complete);
}
#[test]
fn implicit_null_forgiving_access_is_distinct_from_literal_exclamation() {
    let source = r#"@Person!.Name @Factory!()tail @Items![0] @Value! Good day! @End!."#;
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("expression_opaque", "Person!.Name"),
            ("expression_opaque", "Factory!()"),
            ("expression_opaque", "Items![0]"),
            ("expression_opaque", "Value"),
            ("expression_opaque", "End")
        ]
    );
}

#[test]
fn nonconsuming_boundary_evidence_survives_graph_roundtrip_and_rejects_forged_spans() {
    let result = analyze("@Name; tail");
    let roundtrip: nexora_graph::CodeGraph =
        serde_json::from_value(serde_json::to_value(&result.graph).unwrap()).unwrap();
    nexora_graph::GraphIntegrityValidator::validate(&roundtrip).unwrap();
    for change in 0..5 {
        let mut graph = roundtrip.clone();
        let region = graph
            .nodes
            .iter_mut()
            .filter_map(|n| n.lexical_region.as_mut())
            .find(|r| r.mode == "implicit_expression")
            .unwrap();
        let lookahead = region.lookahead_span.as_mut().unwrap();
        match change {
            0 => lookahead.end = lookahead.start,
            1 => lookahead.start.offset -= 1,
            2 => lookahead.end.offset = 100,
            3 => region.closed = false,
            _ => region.closer = Some(*lookahead),
        }
        assert!(
            nexora_graph::GraphIntegrityValidator::validate(&graph).is_err(),
            "mutation {change}"
        );
    }
}

#[test]
fn untyped_component_attribute_values_remain_opaque_even_without_quotes() {
    let result = analyze("<Panel Value=42 />");
    let node = result
        .graph
        .nodes
        .iter()
        .find(|n| {
            n.lexical_region
                .as_ref()
                .is_some_and(|r| r.mode == "markup_tag")
        })
        .unwrap();
    assert!(
        node.regions
            .iter()
            .all(|r| r.status == nexora_core::RegionStatus::Opaque)
    );
    assert!(!result.complete);
}
#[test]
fn email_filter_follows_razor_utf16_letter_digit_neighbors() {
    let source = "é@domain 𐐀@domain @Name";
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("expression_opaque", "domain"),
            ("expression_opaque", "Name")
        ]
    );
}

#[test]
fn inline_templates_preserve_host_followers_with_declared_csharp_dialect() {
    let source =
        r#"@Before() @code { RenderFragment fragment = @<p>@(Hidden()) }</p>; } @(After())"#;
    let result = analyze(source);
    assert_eq!(result.units.len(), 5);
    assert_eq!(
        contents(source, &result)[0],
        ("expression_opaque", "Before()")
    );
    assert_eq!(contents(source, &result)[1].0, "members_opaque");
    assert_eq!(result.units[2].status, nexora_core::RegionStatus::Opaque);
    assert!(result.interpretations.iter().all(|i| i.boundary_closed));
    assert!(contents(source, &result).contains(&("expression_opaque", "Hidden()")));
    assert!(contents(source, &result).contains(&("expression_opaque", "After()")));
    assert!(!result.complete);
    assert!(
        result
            .graph
            .edges
            .iter()
            .all(|e| e.kind != nexora_graph::EdgeKind::Calls)
    );
}
