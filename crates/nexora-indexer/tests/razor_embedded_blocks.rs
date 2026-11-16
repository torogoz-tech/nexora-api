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

#[test]
fn blocks_in_markup_preserve_parent_closers_and_neighboring_expressions() {
    for source in [
        r#"@if (Ready) { <section>@{ var text="</section> }"; } @Name</section> } @(After())"#,
        r#"@code { var view=@<section>@{ var text="</section> }"; } @Name</section>; } @(After())"#,
    ] {
        let result = analyze(source);
        assert!(result.units.iter().all(|u| u.diagnostics.is_empty()));
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .all(|r| r.closed && r.mode != "unknown_tail")
        );
        for value in ["Name", "After()"] {
            assert_eq!(
                result
                    .units
                    .iter()
                    .filter(|u| &source[u.span.start.offset..u.span.end.offset] == value)
                    .count(),
                1
            );
        }
        let body = result
            .units
            .iter()
            .find(|u| u.context.as_deref() == Some("body_opaque"))
            .unwrap();
        assert_eq!(body.lexical_context.as_deref(), Some("razor_body"));
        assert_eq!(body.status, RegionStatus::Opaque);
    }
}

#[test]
fn block_phase_distinguishes_csharp_generics_from_markup_and_protects_literal_code() {
    let source = r#"@{ var values=new List<Item>(); bool less=left<right; var text="<p>@(Fake())</p> }"; <p>@(Real())</p> } @(After())"#;
    let result = analyze(source);
    assert!(result.units.iter().all(|u| u.diagnostics.is_empty()));
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .all(|r| r.closed)
    );
    for value in ["Real()", "After()"] {
        assert_eq!(
            result
                .units
                .iter()
                .filter(|u| &source[u.span.start.offset..u.span.end.offset] == value)
                .count(),
            1
        );
    }
    assert!(
        result
            .units
            .iter()
            .all(|u| &source[u.span.start.offset..u.span.end.offset] != "Fake()")
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
fn truncated_block_retains_partial_evidence_and_pure_csharp_has_no_razor_entry_by_default() {
    let result = analyze("@(Before()) @if (Ready) { <p>@{ var unfinished=1;");
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .any(|r| !r.closed && r.mode == "code_block")
    );
    assert!(!result.complete);
    let pure =
        LanguageDefinition::load(Path::new("../../templates/dotnet-understanding/csharp")).unwrap();
    assert!(
        nexora_lexer::Lexer::new(&pure.lexer)
            .unwrap()
            .tokenize("@{ var x=1; }")
            .is_err()
    );
}

#[test]
fn blocks_in_raw_comment_and_line_carriers_preserve_following_expressions() {
    let mut sources = vec![
        r#"<script>@{ var text="</script> }"; } @(Inside())</script> @(After())"#.to_owned(),
        r#"<style>@{ var text="</style> }"; } @(Inside())</style> @(After())"#.to_owned(),
        r#"@if (Ready) { <!-- @{ var text="--> }"; } @(Inside()) --> } @(After())"#.to_owned(),
    ];
    for newline in ["\n", "\r\n", "\r", "\u{85}", "\u{2028}", "\u{2029}"] {
        sources.push(format!(
            "@if (Ready) {{ @:line @{{ var value=1; }} @(Inside()){newline} }} @(After())"
        ));
    }
    for source in sources {
        let result = analyze(&source);
        assert!(
            result.units.iter().all(|u| u.diagnostics.is_empty()),
            "public case {source}"
        );
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
        for value in ["Inside()", "After()"] {
            assert_eq!(
                result
                    .units
                    .iter()
                    .filter(|u| &source[u.span.start.offset..u.span.end.offset] == value)
                    .count(),
                1
            );
        }
    }
}

#[test]
fn preprocessor_directive_lines_keep_the_block_closer_and_following_unit() {
    for newline in ["\n", "\r\n", "\r", "\u{85}", "\u{2028}", "\u{2029}"] {
        let source =
            format!("@{{ #if DEBUG{newline} var x=1;{newline} #endif{newline} }} @(After())");
        let result = analyze(&source);
        assert!(result.units.iter().all(|u| u.diagnostics.is_empty()));
        assert!(
            result.units[0]
                .tokens
                .iter()
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
        assert_eq!(
            result
                .units
                .iter()
                .filter(|u| &source[u.span.start.offset..u.span.end.offset] == "After()")
                .count(),
            1
        );
        assert!(
            result
                .graph
                .edges
                .iter()
                .all(|e| e.kind != nexora_graph::EdgeKind::Calls)
        );
    }
}
