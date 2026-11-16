use nexora_indexer::CompositionAnalyzer;
use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use std::path::Path;

fn profile(language: &str) -> LanguageDefinition {
    LanguageDefinition::load(&Path::new("../../templates/dotnet-understanding").join(language))
        .unwrap()
}

#[test]
fn raw_interpolations_preserve_code_headers_formats_and_following_declarations() {
    for source in [
        r####"var s = $$"""{"literal":"class Forged {}", "value": {{F("}}", new[] {1, 2})}} }"""; class After {}"####,
        r####"var s = $$"""{{F($""" {value} """)}}"""; class After {}"####,
        r####"var s = $$"""{{value:##}#}}"""; class After {}"####,
        r####"var s = $$$""""{{literal}} {{{value}}} """ inside""""; class After {}"####,
    ] {
        let p = profile("csharp");
        let tokens = Lexer::new(&p.lexer).unwrap().tokenize(source).unwrap();
        assert!(tokens.iter().flat_map(|t| &t.regions).all(|r| r.closed));
        assert!(
            tokens
                .iter()
                .flat_map(|t| &t.regions)
                .any(|r| r.mode == "raw_interpolation")
        );
        let result = CompositionAnalyzer::new(p, vec![])
            .unwrap()
            .analyze_source("public.cs", source)
            .unwrap();
        assert!(
            result
                .graph
                .nodes
                .iter()
                .any(|n| n.kind == nexora_graph::NodeKind::Class && n.name == "After")
        );
        assert!(result.graph.nodes.iter().all(|n| n.name != "Forged"));
    }
}

#[test]
fn razor_members_and_body_dialects_keep_raw_text_out_of_markup_and_implicit_code() {
    for source in [
        r####"@code { string Text = $$"""<p>@fake {single} {{F("}}")}}</p>"""; }<p>After</p>"####,
        r####"@{var text = $$"""{{F(@<em>value</em>)}}""";}<p>After</p>"####,
    ] {
        let result = CompositionAnalyzer::new(profile("razor"), vec![profile("csharp")])
            .unwrap()
            .analyze_source("public.razor", source)
            .unwrap();
        assert!(
            result
                .units
                .iter()
                .flat_map(|u| &u.tokens)
                .flat_map(|t| &t.regions)
                .all(|r| r.closed)
        );
        assert!(
            result
                .units
                .iter()
                .flat_map(|u| &u.tokens)
                .flat_map(|t| &t.regions)
                .any(|r| r.mode == "c_raw_interpolation")
        );
        assert!(
            result
                .units
                .iter()
                .flat_map(|u| &u.tokens)
                .flat_map(|t| &t.regions)
                .all(|r| r.mode != "unknown_tail")
        );
        assert_eq!(
            result
                .units
                .iter()
                .filter(|u| u.language.0 == "csharp")
                .count(),
            1
        );
    }
}

#[test]
fn unclosed_raw_holes_remain_explicitly_incomplete() {
    let source = r####"var s = $$"""{{value"####;
    let result = CompositionAnalyzer::new(profile("csharp"), vec![])
        .unwrap()
        .analyze_source("public.cs", source)
        .unwrap();
    assert!(!result.complete);
    assert!(
        result.units[0]
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .any(|r| r.mode == "raw_interpolation" && !r.closed)
    );
}

#[test]
fn raw_interpolation_comments_preserve_all_declared_line_endings() {
    let p = profile("csharp");
    for name in ["line_comment", "c_line_comment"] {
        assert!(
            p.lexer.regions[name]
                .alternate_closes
                .contains(&"\u{85}".into())
        );
        assert!(!p.lexer.regions[name].alternate_closes.contains(&" ".into()));
    }
    let lexer = Lexer::new(&p.lexer).unwrap();
    for newline in ["\n", "\r\n", "\r", "\u{85}", "\u{2028}", "\u{2029}"] {
        let source = r####"var s = $$"""{{F( // fake }} \" BREAK42)}}"""; class After {}"####
            .replace("BREAK", newline);
        let tokens = lexer.tokenize(&source).unwrap();
        assert!(tokens.iter().flat_map(|t| &t.regions).all(|r| r.closed));
        assert!(tokens.iter().any(|t| t.text == "After"));
    }
}
