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
fn control_header_and_markup_body_have_separate_ranges_and_keep_literal_braces() {
    let source =
        r#"@if (flag) { <div title="@(F("}"))">literal } { <span>@Name</span></div> } @(After())"#;
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("control_header_opaque", " (flag) "),
            ("expression_opaque", r#"F("}")"#),
            ("expression_opaque", "Name"),
            ("expression_opaque", "After()")
        ]
    );
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .all(|r| r.closed)
    );
    assert!(!result.complete);
}
#[test]
fn csharp_generic_and_comparison_statements_do_not_open_markup_elements() {
    let source = r#"@if (flag) { var list = new List<Item>(); bool less = left < right; <p>@list.Count</p> } @(After())"#;
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("control_header_opaque", " (flag) "),
            ("expression_opaque", "list.Count"),
            ("expression_opaque", "After()")
        ]
    );
    let regions: Vec<_> = result.units[0]
        .tokens
        .iter()
        .flat_map(|t| &t.regions)
        .collect();
    assert_eq!(
        regions.iter().filter(|r| r.mode == "body_element").count(),
        1
    );
    assert_eq!(
        regions
            .iter()
            .filter(|r| r.mode == "csharp_statement")
            .count(),
        2
    );
    assert!(regions.iter().all(|r| r.closed));
}
#[test]
fn nested_controls_comments_and_void_elements_do_not_lose_following_code() {
    let source = r#"@foreach (var item in items) { @* } <Fake> *@ if (item.Ready) { <input value="@item.Name" /><br><span>@(Format("}"))</span> } } @(After())"#;
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("control_header_opaque", " (var item in items) "),
            ("control_header_opaque", " (item.Ready) "),
            ("expression_opaque", "item.Name"),
            ("expression_opaque", r#"Format("}")"#),
            ("expression_opaque", "After()")
        ]
    );
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .all(|r| r.closed)
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
fn truncated_markup_inside_a_control_is_partial_instead_of_a_completed_control() {
    for source in [
        "@if (flag) { <div>literal } @(Name)",
        "@if (flag) { <div></span> } @(After())",
    ] {
        let result = analyze(source);
        assert!(!result.complete);
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .any(|r| !r.closed)
        );
    }
}
#[test]
fn raw_elements_directly_in_controls_keep_literal_tags_and_following_statements() {
    for raw in [
        r#"<script>const x="<fake>"; const brace="}"; @(Value)</script>"#,
        r#"<style>.x::before { content: "<fake> }"; } @(Value)</style>"#,
    ] {
        let source = format!("@if (flag) {{ {raw} var next = 1; <p>@Name</p> }} @(After())");
        let result = analyze(&source);
        assert_eq!(
            contents(&source, &result),
            [
                ("control_header_opaque", " (flag) "),
                ("expression_opaque", "Value"),
                ("expression_opaque", "Name"),
                ("expression_opaque", "After()")
            ]
        );
        let regions: Vec<_> = result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .collect();
        assert!(regions.iter().all(|r| r.closed));
        assert_eq!(
            regions.iter().filter(|r| r.mode == "body_element").count(),
            1
        );
        assert_eq!(
            regions
                .iter()
                .filter(|r| r.mode == "csharp_statement")
                .count(),
            1
        );
    }
}
#[test]
fn parenthesized_control_headers_keep_inner_delimiters_and_using_imports_distinct() {
    for header in [
        "for (var i = 0; i < 3; i++) ",
        "while (Check(\"{\")) ",
        "lock (gate) ",
        "using (var resource = Open()) ",
    ] {
        let source = format!("@using Public.Example\n@{header}{{ <p>@Name</p> }} @(After())");
        let result = analyze(&source);
        assert_eq!(
            contents(&source, &result),
            [
                (
                    "control_header_opaque",
                    &header[header.find(' ').unwrap()..]
                ),
                ("expression_opaque", "Name"),
                ("expression_opaque", "After()")
            ]
        );
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
        assert!(result.units[0].tokens.iter().any(|t| t.kind == "directive"));
    }
}

#[test]
fn simple_switch_labels_and_markup_lines_preserve_following_regions() {
    let source = "@switch (value) { case 1: <p>@Name</p> break; default: @:literal @(Other())\n break; } @(After())";
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [
            ("control_header_opaque", " (value) "),
            ("expression_opaque", "Name"),
            ("expression_opaque", "Other()"),
            ("expression_opaque", "After()")
        ]
    );
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .all(|r| r.closed)
    );
}
#[test]
fn doubled_at_in_element_headers_keeps_following_attributes_and_siblings() {
    for source in [
        "@if (flag) { <div data-at=@@ data-value=@(Value)>@Name</div> } @(After())",
        "@if (flag) { <Panel data-at=@@ data-value=@(Value) /><p>@Name</p> } @(After())",
        "@if (flag) { <div><span data-at=@@ data-value=@(Value)>@Name</span></div> } @(After())",
        "@if (flag) { <div data-at=@@@Value>@Name</div> } @(After())",
    ] {
        let result = analyze(source);
        assert_eq!(
            contents(source, &result),
            [
                ("control_header_opaque", " (flag) "),
                ("expression_opaque", "Value"),
                ("expression_opaque", "Name"),
                ("expression_opaque", "After()")
            ]
        );
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
    }
}
