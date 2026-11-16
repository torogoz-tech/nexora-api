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
    .analyze_source("public.razor", source)
    .unwrap()
}
fn contents<'a>(source: &'a str, result: &CompositionResult) -> Vec<&'a str> {
    result
        .units
        .iter()
        .skip(1)
        .map(|u| &source[u.span.start.offset..u.span.end.offset])
        .collect()
}
#[test]
fn template_literals_keep_member_braces_semicolons_and_following_code_owned() {
    let source =
        "@Before() @code { var view = @<p>; class Ghost { } @Name</p>; int after = 2; } @(After())";
    let result = analyze(source);
    let values = contents(source, &result);
    assert_eq!(
        values,
        [
            "Before()",
            " var view = @<p>; class Ghost { } @Name</p>; int after = 2; ",
            "Name",
            "After()"
        ]
    );
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .all(|r| r.closed)
    );
    assert_eq!(result.units[2].status, nexora_core::RegionStatus::Opaque);
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
fn nested_templates_and_attribute_expressions_keep_csharp_string_closers_protected() {
    let source = r#"@code { var view = @<div title="@(Format("</div>"))">@(@<span>@Name</span>)</div>; } @(After())"#;
    let result = analyze(source);
    let values = contents(source, &result);
    for value in [
        r#"Format("</div>")"#,
        "@<span>@Name</span>",
        "Name",
        "After()",
    ] {
        assert!(values.contains(&value), "missing public fragment {value}");
    }
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .all(|r| r.closed)
    );
    assert_eq!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .filter(|r| r.mode == "body_element")
            .count(),
        2
    );
}
#[test]
fn implicit_arguments_in_templates_cannot_close_the_markup_from_a_csharp_string() {
    let source = r#"@code { var view = @<p>@Format("</p>")</p>; } @(After())"#;
    let result = analyze(source);
    let values = contents(source, &result);
    assert!(values.contains(&r#"Format("</p>")"#));
    assert!(values.contains(&"After()"));
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .all(|r| r.closed)
    );
}
#[test]
fn void_self_closed_and_custom_tags_keep_template_boundaries_and_followers() {
    for markup in [
        "@<input value=\"@(Name)\" />",
        "@<br>",
        "@<Panel value=\"@(Name)\" />",
        "@<input-card>@Name</input-card>",
    ] {
        let source = format!("@code {{ var view = {markup}; }} @(After())");
        let result = analyze(&source);
        assert!(contents(&source, &result).contains(&"After()"));
        if markup.contains("Name") {
            assert!(contents(&source, &result).contains(&"Name"));
        }
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
    }
}
#[test]
fn truncated_or_unsupported_template_openers_remain_explicitly_partial_or_unknown() {
    for template in ["@<p>@Name", "@<p></other>", "@<:unsupported>@(Hidden())"] {
        let source = format!("@Before() @code {{ var view = {template}; }} @(After())");
        let result = analyze(&source);
        assert_eq!(contents(&source, &result)[0], "Before()");
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .any(|r| !r.closed || r.mode == "unknown_tail")
        );
        assert!(!result.complete);
    }
}
#[test]
fn template_text_in_csharp_strings_stays_literal_and_pure_csharp_remains_strict() {
    let source = r#"@code { var text="@<p>@(Fake())</p>"; } @(After())"#;
    let result = analyze(source);
    assert_eq!(
        contents(source, &result),
        [r#" var text="@<p>@(Fake())</p>"; "#, "After()"]
    );
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .all(|r| r.mode != "body_element")
    );
    let pure =
        LanguageDefinition::load(Path::new("../../templates/dotnet-understanding/csharp")).unwrap();
    assert!(
        nexora_lexer::Lexer::new(&pure.lexer)
            .unwrap()
            .tokenize("@<p>value</p>")
            .is_err()
    );
}
#[test]
fn raw_script_and_style_templates_keep_literal_tags_and_following_members() {
    for raw in [
        r#"@<script>const s="<fake> }"; @(Value)</script>"#,
        r#"@<style>.x { content:"<fake> }"; } @(Value)</style>"#,
        r#"@<ScRiPt>const s="<fake>"; @(Value)</sCrIpT>"#,
    ] {
        let source = format!("@code {{ var template = {raw}; int after=1; }} @(After())");
        let result = analyze(&source);
        let values = contents(&source, &result);
        assert!(values.contains(&"Value"));
        assert!(values.contains(&"After()"));
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .all(|r| r.mode != "body_element")
        );
    }
}
