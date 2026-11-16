use nexora_indexer::{CompositionAnalyzer, CompositionResult};
use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use std::path::Path;

fn model() -> LanguageDefinition {
    LanguageDefinition::load(Path::new("../../templates/dotnet-understanding/csharp")).unwrap()
}
fn analyze(source: &str) -> CompositionResult {
    CompositionAnalyzer::new(model(), vec![])
        .unwrap()
        .analyze_source("source.cs", source)
        .unwrap()
}

#[test]
fn verbatim_regular_and_character_literals_preserve_following_code() {
    let source = r#"var a = @"a""{notCode}"; var b = "a\"}"; var c = '\''; class After {}"#;
    let tokens = Lexer::new(&model().lexer)
        .unwrap()
        .tokenize(source)
        .unwrap();
    let strings: Vec<_> = tokens
        .iter()
        .filter(|t| ["string", "verbatim_string", "character"].contains(&t.kind.as_str()))
        .collect();
    assert_eq!(strings.len(), 3);
    assert_eq!(strings[0].text, r#"@"a""{notCode}""#);
    assert_eq!(strings[1].text, r#""a\"}""#);
    assert_eq!(strings[2].text, r"'\''");
    assert!(strings.iter().all(|t| t.regions.iter().all(|r| r.closed)));
    assert!(tokens.iter().any(|t| t.text == "After"));
}

#[test]
fn interpolation_distinguishes_literal_braces_nested_code_and_quoted_delimiters() {
    for source in [
        r#"var s = $"{{literal}} {new Box { Value = F("}", new[] { 1, 2 }) }}"; class After {}"#,
        r#"var s = $@"""{{literal}}"" {F(@"}""}")}"; class After {}"#,
        r#"var s = @$"{{literal}} {F($"{G("}")}")}"; class After {}"#,
    ] {
        let tokens = Lexer::new(&model().lexer)
            .unwrap()
            .tokenize(source)
            .unwrap();
        let literal = tokens
            .iter()
            .find(|t| ["interpolated_string", "interpolated_verbatim"].contains(&t.kind.as_str()))
            .unwrap();
        assert!(literal.regions.iter().all(|r| r.closed), "{source}");
        assert_eq!(
            literal
                .regions
                .iter()
                .filter(|r| r.mode == "interpolation")
                .count(),
            if source.contains("G(") { 2 } else { 1 }
        );
        assert!(tokens.iter().any(|t| t.text == "After"));
    }
}

#[test]
fn raw_quote_fences_and_comments_do_not_interpret_embedded_language_markers() {
    let source = "// $\"{fake}\"\u{85}var a = \"\"\"<p>\"x\" { data }</p>\"\"\"; /* $\"{fake}\" */ var b = \"\"\"\"a\"\"\"b\"\"\"\"; class After {}";
    let tokens = Lexer::new(&model().lexer)
        .unwrap()
        .tokenize(source)
        .unwrap();
    let raw: Vec<_> = tokens.iter().filter(|t| t.kind == "raw_string").collect();
    assert_eq!(raw.len(), 2);
    assert!(
        raw.iter()
            .all(|t| t.regions.len() == 1 && t.regions[0].closed)
    );
    assert!(tokens.iter().any(|t| t.text == "After"));
    assert!(
        !tokens
            .iter()
            .flat_map(|t| &t.regions)
            .any(|r| r.mode == "interpolation")
    );
}

#[test]
fn excessive_raw_opening_runs_are_explicit_and_truncation_is_partial() {
    let lexer = Lexer::new(&model().lexer).unwrap();
    assert!(lexer.tokenize(r#"var s = $$"""{{{{value}}""";"#).is_err());
    for source in [
        r#"var s = @"unfinished"#,
        r#"var s = $"{F("x")"#,
        "var s = \"\"\"unfinished",
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
    let result = analyze("class C { string value = @\"a\"\"b\"; }");
    assert!(
        !result.complete,
        "lexical boundaries alone do not establish semantic coverage"
    );
    assert!(
        !result
            .graph
            .edges
            .iter()
            .any(|e| e.kind == nexora_graph::EdgeKind::Calls)
    );
}

#[test]
fn adjacent_operators_cannot_consume_string_prefixes_or_comment_openers() {
    let source = r#"var a=$"{F()}"; var b=@"x""y"; var c=1+/* } " fake */2;// fake
class After {}"#;
    let tokens = Lexer::new(&model().lexer)
        .unwrap()
        .tokenize(source)
        .unwrap();
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == "interpolated_string" && t.text == r#"$"{F()}""#)
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == "verbatim_string" && t.text == r#"@"x""y""#)
    );
    assert!(tokens.iter().any(|t| t.kind == "block_comment"));
    assert!(tokens.iter().any(|t| t.kind == "line_comment"));
    assert!(tokens.iter().any(|t| t.text == "After"));
}

#[test]
fn byte_order_mark_and_comment_line_endings_preserve_original_source_offsets() {
    let lexer = Lexer::new(&model().lexer).unwrap();
    for ending in ["\r", "\n", "\r\n", "\u{85}", "\u{2028}", "\u{2029}"] {
        let source = format!("\u{feff}// fake $\"{{x}}\"{ending}class After {{}}");
        let tokens = lexer.tokenize(&source).unwrap();
        let after = tokens.iter().find(|t| t.text == "After").unwrap();
        assert_eq!(after.span.start.offset, source.find("After").unwrap());
        assert!(
            !tokens
                .iter()
                .flat_map(|t| &t.regions)
                .any(|r| r.mode == "interpolation")
        );
    }
}

#[test]
fn interpolation_format_is_literal_body_and_expression_is_a_separate_header_span() {
    let source = r#"var s=$"{DateTime.Now,12:yyyy 'x'} {((flag ? "a" : "b"))}"; class After {}"#;
    let tokens = Lexer::new(&model().lexer)
        .unwrap()
        .tokenize(source)
        .unwrap();
    let literal = tokens
        .iter()
        .find(|t| t.kind == "interpolated_string")
        .unwrap();
    assert!(literal.regions.iter().all(|r| r.closed));
    let format = literal
        .regions
        .iter()
        .find(|r| r.mode == "interpolation" && !r.self_closed)
        .unwrap();
    let header = format.header_content_span.unwrap();
    assert_eq!(
        &source[header.start.offset..header.end.offset],
        "DateTime.Now,12"
    );
    assert_eq!(
        &source[format.content_span.start.offset..format.content_span.end.offset],
        "yyyy 'x'"
    );
    assert!(
        literal
            .regions
            .iter()
            .any(|r| r.mode == "interpolation" && r.self_closed)
    );
    assert!(tokens.iter().any(|t| t.text == "After"));
}

#[test]
fn interpolation_headers_shield_comments_across_lines_and_verbatim_formats_keep_quotes_literal() {
    let lexer = Lexer::new(&model().lexer).unwrap();
    for source in [
        "var s=$\"{F( // fake } \\\"\n42)}\"; class After {}",
        r#"var s=$@"{value:yyyy ""x""}"; class After {}"#,
    ] {
        let tokens = lexer.tokenize(source).unwrap();
        let literal = tokens
            .iter()
            .find(|t| ["interpolated_string", "interpolated_verbatim"].contains(&t.kind.as_str()))
            .unwrap();
        assert!(literal.regions.iter().all(|r| r.closed));
        assert!(tokens.iter().any(|t| t.text == "After"));
    }
}
