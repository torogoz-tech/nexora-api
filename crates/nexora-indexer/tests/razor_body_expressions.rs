use nexora_core::RegionStatus;
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
fn expressions<'a>(result: &CompositionResult, source: &'a str) -> Vec<&'a str> {
    result
        .units
        .iter()
        .filter(|u| u.context.as_deref() == Some("expression_opaque"))
        .map(|u| &source[u.span.start.offset..u.span.end.offset])
        .collect()
}

#[test]
fn body_transitions_have_their_own_input_and_never_require_statement_semicolons() {
    for (source, expression) in [
        ("@if (Ready) { @Value } @(After())", "Value"),
        ("@if (Ready) { @State.Name } @(After())", "State.Name"),
        (
            "@foreach(var item in Items) { @Format(item) } @(After())",
            "Format(item)",
        ),
        ("@{ @Items[0] } @(After())", "Items[0]"),
        ("@if (Ready) { { @Value } } @(After())", "Value"),
    ] {
        let result = analyze(source);
        assert_eq!(
            expressions(&result, source),
            [expression, "After()"],
            "{source}"
        );
        assert!(
            result
                .units
                .iter()
                .flat_map(|u| &u.tokens)
                .flat_map(|t| &t.regions)
                .all(|r| r.closed),
            "{source}"
        );
        assert!(
            result
                .units
                .iter()
                .all(|u| u.status != RegionStatus::Partial),
            "{source}"
        );
        nexora_graph::GraphIntegrityValidator::validate(&result.graph).unwrap();
    }
}

#[test]
fn statements_controls_markup_and_branches_can_follow_body_expressions() {
    for (source, expected) in [
        (
            "@if (Ready){@Name var local=1; if(Other){@More}<b>@Tail</b>} @(After())",
            vec!["Name", "More", "Tail", "After()"],
        ),
        (
            "@if(Ready){ @Get(\"}\") } else { @Fallback } @(After())",
            vec!["Get(\"}\")", "Fallback", "After()"],
        ),
        (
            "@if(Ready){ @First @Second } @(After())",
            vec!["First", "Second", "After()"],
        ),
    ] {
        let result = analyze(source);
        assert_eq!(expressions(&result, source), expected, "{source}");
        assert!(
            result
                .units
                .iter()
                .flat_map(|u| &u.tokens)
                .flat_map(|t| &t.regions)
                .all(|r| r.closed),
            "{source}"
        );
    }
}

#[test]
fn escaped_identifiers_inside_statements_and_literal_text_do_not_start_razor_units() {
    let source = r#"@if(Ready){var @class=1; Use(@class); var text="@No"; /* @Hidden */ @Visible} @(After())"#;
    let result = analyze(source);
    assert_eq!(expressions(&result, source), ["Visible", "After()"]);
    assert!(
        result
            .units
            .iter()
            .flat_map(|u| &u.tokens)
            .flat_map(|t| &t.regions)
            .all(|r| r.closed)
    );
    let root = Path::new("../../templates/dotnet-understanding/csharp");
    let pure = CompositionAnalyzer::new(LanguageDefinition::load(root).unwrap(), vec![])
        .unwrap()
        .analyze_source(
            "public.cs",
            "class Valid { void Work(){var @class=1; Use(@class);} }",
        )
        .unwrap();
    assert_eq!(pure.units.len(), 1);
}

#[test]
fn bare_incomplete_statements_and_unmodeled_transitions_remain_uncertain() {
    let source = "@if(Ready) { Bare } @(After())";
    let result = analyze(source);
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .any(|r| r.mode == "csharp_statement" && !r.closed)
    );
    assert_eq!(expressions(&result, source), ["After()"]);
    assert_eq!(result.units[0].status, RegionStatus::Partial);
    for source in ["@if(Ready) { @await Task() }", "@if(Ready) { @ }"] {
        let result = analyze(source);
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .any(|r| r.mode == "unknown_tail")
        );
        assert!(!result.complete);
        assert!(expressions(&result, source).is_empty());
    }
}

#[test]
fn unicode_body_expression_spans_and_unconsumed_parent_closers_remain_original() {
    let source = "<p>é</p>\n@if(Ready){@नाम} @(After())";
    let result = analyze(source);
    assert_eq!(expressions(&result, source), ["नाम", "After()"]);
    let unit = result
        .units
        .iter()
        .find(|u| &source[u.span.start.offset..u.span.end.offset] == "नाम")
        .unwrap();
    assert_eq!(unit.span.start.offset, source.find("नाम").unwrap());
    let region = result.units[0]
        .tokens
        .iter()
        .flat_map(|t| &t.regions)
        .find(|r| r.mode == "implicit_expression")
        .unwrap();
    assert_eq!(region.opener.end, unit.span.start);
    assert_eq!(
        region.lookahead_span.unwrap().start.offset,
        source.find("} @(").unwrap()
    );
}

#[test]
fn reserved_body_words_do_not_become_expression_units_but_longer_names_do() {
    for word in [
        "namespace",
        "page",
        "inherits",
        "case",
        "default",
        "attribute",
        "inject",
        "layout",
        "implements",
        "typeparam",
        "rendermode",
        "preservewhitespace",
    ] {
        for source in [
            format!("@if(Ready){{@{word}}} @(After())"),
            format!("@{{ @{word} }} @(After())"),
        ] {
            let result = analyze(&source);
            assert!(!expressions(&result, &source).contains(&word), "{source}");
            assert!(
                result
                    .units
                    .iter()
                    .flat_map(|u| &u.tokens)
                    .flat_map(|t| &t.regions)
                    .any(|r| r.mode == "unknown_tail"),
                "{source}"
            );
            assert!(!result.complete);
        }
        let source = format!("@if(Ready){{@{word}Value}} @(After())");
        let result = analyze(&source);
        assert_eq!(
            expressions(&result, &source),
            [format!("{word}Value"), "After()".into()]
        );
        assert!(
            result
                .units
                .iter()
                .flat_map(|u| &u.tokens)
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
    }
}

#[test]
fn reserved_prefixes_follow_the_declared_unicode_identifier_parts() {
    for suffix in [
        "\u{ad}Value",
        "\u{200b}Value",
        "\u{301}Value",
        "\u{203f}Value",
    ] {
        let source = format!("@if(Ready){{@namespace{suffix}}} @(After())");
        let result = analyze(&source);
        assert_eq!(
            expressions(&result, &source),
            [format!("namespace{suffix}"), "After()".into()]
        );
        assert!(
            result
                .units
                .iter()
                .flat_map(|u| &u.tokens)
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
    }
}
