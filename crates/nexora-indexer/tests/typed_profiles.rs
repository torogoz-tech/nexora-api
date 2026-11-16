use nexora_graph::{EdgeKind, NodeKind};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use std::path::Path;
fn analyze(id: &str, file: &str) -> nexora_indexer::AnalysisResult {
    let language = LanguageDefinition::load(&Path::new("../../languages").join(id)).unwrap();
    Indexer::new(language)
        .analyze(&Path::new("../../tests/fixtures/typed-profiles").join(file))
        .unwrap()
}
#[test]
fn typed_declarations_are_real_nodes_with_explicit_opaque_interiors() {
    let r = analyze("typescript", "declarations.ts");
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    for (name, kind) in [
        ("Item", "Interface"),
        ("ItemId", "TypeAlias"),
        ("Status", "Enum"),
        ("Cart", "Class"),
        ("fetchItem", "Function"),
        ("double", "Function"),
        ("config", "Variable"),
    ] {
        assert!(
            r.graph
                .nodes
                .iter()
                .any(|n| n.name == name && format!("{:?}", n.kind) == kind),
            "{name}"
        );
    }
    assert!(!r.symbols.complete);
    assert!(
        r.graph
            .syntax_coverage
            .values()
            .any(|c| c.opaque_regions > 0)
    );
    assert!(!r.graph.nodes.iter().any(|n| n.name == "local"));
    assert!(!r.graph.edges.iter().any(|e| e.kind == EdgeKind::Calls));
}
#[test]
fn tsx_and_jsx_preserve_nested_markup_without_claiming_react_semantics() {
    for (id, file) in [("tsx", "component.tsx"), ("jsx", "component.jsx")] {
        let r = analyze(id, file);
        assert!(r.diagnostics.is_empty(), "{id}: {:?}", r.diagnostics);
        assert!(
            r.graph
                .nodes
                .iter()
                .any(|n| n.name == "Button" && n.kind == NodeKind::Function)
        );
        let button = r.ast.nodes.iter().find(|n| n.name == "Button").unwrap();
        let body = button.regions.iter().find(|r| r.capture == "body").unwrap();
        assert_eq!(body.status, nexora_core::RegionStatus::Opaque);
        let text = &r.source.content[body.span.start.offset..body.span.end.offset];
        assert!(text.contains("<button") && text.contains("</button>"));
        assert!(!r.symbols.complete);
        assert!(!r.graph.nodes.iter().any(|n| n.name == "setCount"));
    }
}
#[test]
fn missing_delimiters_are_reported_and_trivia_ranges_remain_exact() {
    let r = analyze("tsx", "incomplete.tsx");
    assert!(!r.diagnostics.is_empty());
    let good = analyze("typescript", "declarations.ts");
    assert_eq!(
        good.tokens
            .iter()
            .map(|t| t.text.as_str())
            .collect::<String>(),
        good.source.content
    );
    for n in &good.ast.nodes {
        let mut p = nexora_core::SourcePosition::default();
        p.advance(&good.source.content[..n.span.start.offset]);
        assert_eq!(p, n.span.start);
    }
}
#[test]
fn additional_graph_vocabulary_requires_a_new_schema() {
    let mut l = LanguageDefinition::load(Path::new("../../languages/typescript")).unwrap();
    l.graph.schema_version = 5;
    assert!(l.validate().is_err());
}
#[test]
fn html_fragments_and_css_blocks_preserve_source_with_honest_coverage() {
    for (id, file) in [("html", "document.html"), ("css", "styles.css")] {
        let r = analyze(id, file);
        assert!(r.diagnostics.is_empty(), "{id}: {:?}", r.diagnostics);
        assert!(!r.ast.nodes.is_empty());
        assert!(!r.symbols.complete);
        assert!(r.symbols.symbols.is_empty());
        assert_eq!(
            r.tokens.iter().map(|t| t.text.as_str()).collect::<String>(),
            r.source.content
        );
        assert!(
            r.graph
                .syntax_coverage
                .values()
                .any(|c| c.opaque_regions > 0)
        );
    }
}
fn analyze_text(
    id: &str,
    source: &str,
    name: &str,
) -> Result<nexora_indexer::AnalysisResult, nexora_core::NexoraError> {
    let l = LanguageDefinition::load(&Path::new("../../languages").join(id)).unwrap();
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-web-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let p = root.join(format!("source.{}", l.manifest.extensions[0]));
    std::fs::write(&p, source).unwrap();
    let r = Indexer::new(l).analyze(&p);
    std::fs::remove_dir_all(root).unwrap();
    r
}
#[test]
fn jsx_text_never_creates_symbols_or_hides_the_next_declaration() {
    for (idx, text) in [
        "Don't stop {user}",
        "https://example.com",
        "hello; const leaked = 1;",
    ]
    .iter()
    .enumerate()
    {
        for id in ["tsx", "jsx"] {
            let source = format!("const Button = () => <div>{text}</div>;\nconst after = 2;");
            let r = analyze_text(id, &source, &format!("text-{id}-{idx}")).unwrap();
            assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
            assert!(r.graph.nodes.iter().any(|n| n.name == "after"));
            assert!(!r.graph.nodes.iter().any(|n| n.name == "leaked"));
        }
    }
}
#[test]
fn template_interpolation_is_nested_without_declaring_literal_names() {
    let source = "const t = `outer ${`fake; const leaked = 1;`} end`;\nconst after = 2;";
    for id in ["typescript", "tsx", "jsx"] {
        let r = analyze_text(id, source, &format!("template-{id}")).unwrap();
        assert!(!r.graph.nodes.iter().any(|n| n.name == "leaked"));
        assert!(r.graph.nodes.iter().any(|n| n.name == "after"));
        assert!(r.graph.nodes.iter().any(|n| n.lexical_region.is_some()));
    }
}
#[test]
fn semicolonless_declarations_are_diagnosed_instead_of_absorbing_their_sibling() {
    for (i, source) in [
        "type A = string\nconst after = 2;",
        "const first = 1\nconst after = 2;",
        "const first = () => 1\nconst after = 2;",
    ]
    .iter()
    .enumerate()
    {
        let r = analyze_text("typescript", source, &format!("asi-{i}")).unwrap();
        assert!(!r.diagnostics.is_empty());
        assert!(r.graph.nodes.iter().any(|n| n.name == "after"));
    }
}
#[test]
fn new_declaration_kinds_are_version_gated_in_reference_targets_too() {
    let mut l = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    l.graph
        .symbol_references
        .values_mut()
        .next()
        .unwrap()
        .target_kinds
        .push("TypeAlias".into());
    assert!(l.validate().is_err());
}
#[test]
fn generic_type_arguments_are_not_mistaken_for_markup_in_jsx_profiles() {
    let source = "const state = useState<RemoteItem>(null);\nconst Button = () => <button>Ready</button>;\nconst after = a < b;";
    let r = analyze_text("tsx", source, "generic-vs-markup").unwrap();
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    assert!(r.graph.nodes.iter().any(|n| n.name == "state"));
    assert!(r.graph.nodes.iter().any(|n| n.name == "after"));
}
#[test]
fn lexer_contexts_match_previous_significant_token_and_are_versioned() {
    let value = serde_json::json!({"schema_version":2,"identifier_kind":"word","keywords":["LET"],"tokens":[{"kind":"space","pattern":"\\s+","trivia":true},{"kind":"comment","pattern":"/\\*[^*]*\\*/","trivia":true},{"kind":"special","pattern":"@","after":[{"text":"LET","kind":"keyword"}],"at_start":false},{"kind":"punctuation","pattern":"@"},{"kind":"word","pattern":"[A-Z]+"}]});
    let mut definition: nexora_language::LexerDefinition = serde_json::from_value(value).unwrap();
    let tokens = nexora_lexer::Lexer::new(&definition)
        .unwrap()
        .tokenize("@ LET /* gap */ @")
        .unwrap();
    let kinds: Vec<_> = tokens
        .iter()
        .filter(|t| !t.trivia)
        .map(|t| t.kind.as_str())
        .collect();
    assert_eq!(kinds, vec!["punctuation", "keyword", "special"]);
    definition.tokens[2].at_start = true;
    let tokens = nexora_lexer::Lexer::new(&definition)
        .unwrap()
        .tokenize("@")
        .unwrap();
    assert_eq!(tokens[0].kind, "special");
    definition.schema_version = 1;
    assert!(nexora_lexer::Lexer::new(&definition).is_err());
    definition.schema_version = 2;
    definition.tokens[2].after[0].kind = Some("missing".into());
    assert!(nexora_lexer::Lexer::new(&definition).is_err());
}
#[test]
fn at_start_false_is_respected_without_a_previous_token_filter() {
    let definition:nexora_language::LexerDefinition=serde_json::from_value(serde_json::json!({"schema_version":2,"identifier_kind":"word","keywords":[],"tokens":[{"kind":"special","pattern":"@","at_start":false},{"kind":"fallback","pattern":"@"},{"kind":"space","pattern":"\\s+","trivia":true},{"kind":"word","pattern":"[a-z]+"}]})).unwrap();
    let r = nexora_lexer::Lexer::new(&definition)
        .unwrap()
        .tokenize("@ word @")
        .unwrap();
    let kinds: Vec<_> = r
        .iter()
        .filter(|t| !t.trivia)
        .map(|t| t.kind.as_str())
        .collect();
    assert_eq!(kinds, vec!["fallback", "word", "special"]);
}
#[test]
fn opaque_literal_boundaries_use_the_stack_and_enforce_depth_limits() {
    let plain = analyze_text(
        "typescript",
        "const t = `plain; const leaked = 1;`; const after = 2;",
        "plain-template",
    )
    .unwrap();
    assert!(plain.graph.nodes.iter().any(|n| n.name == "after"));
    assert!(!plain.graph.nodes.iter().any(|n| n.name == "leaked"));
    let deep = format!(
        "const Button = () => {}hello; const leaked = 1;{}; const after = 2;",
        "<div>".repeat(5),
        "</div>".repeat(5)
    );
    let deep_result = analyze_text("tsx", &deep, "deep-markup").unwrap();
    assert!(deep_result.diagnostics.is_empty());
    assert!(deep_result.graph.nodes.iter().any(|n| n.name == "after"));
    assert!(!deep_result.graph.nodes.iter().any(|n| n.name == "leaked"));
    let mut language = LanguageDefinition::load(Path::new("../../languages/tsx")).unwrap();
    language.lexer.max_region_depth = 4;
    assert!(
        nexora_lexer::Lexer::new(&language.lexer)
            .unwrap()
            .tokenize(&deep)
            .is_err()
    );
}
#[test]
fn regular_expression_literal_text_does_not_declare_fake_top_level_symbols() {
    for id in ["typescript", "tsx", "jsx"] {
        let r = analyze_text(
            id,
            "const pattern = /; const leaked = 1; /; const after = 2;",
            &format!("regex-literal-{id}"),
        )
        .unwrap();
        assert!(r.diagnostics.is_empty(), "{id}: {:?}", r.diagnostics);
        assert!(!r.symbols.symbols.iter().any(|s| s.name == "leaked"));
        assert!(r.symbols.symbols.iter().any(|s| s.name == "after"));
    }
}
#[test]
fn context_mismatch_guard_respects_longest_match_and_does_not_guess_ambiguous_literals() {
    let d:nexora_language::LexerDefinition=serde_json::from_value(serde_json::json!({"schema_version":4,"identifier_kind":"word","keywords":["LET"],"tokens":[{"kind":"guard","pattern":"@","after":[{"text":"LET"}],"at_start":false,"reject_on_context_mismatch":true},{"kind":"longer","pattern":"@LONG"},{"kind":"fallback","pattern":"@"},{"kind":"space","pattern":"\\s+","trivia":true},{"kind":"word","pattern":"[A-Z]+"}]})).unwrap();
    let l = nexora_lexer::Lexer::new(&d).unwrap();
    assert!(l.tokenize("@").is_err());
    assert_eq!(l.tokenize("@LONG").unwrap()[0].kind, "longer");
    assert_eq!(l.tokenize("LET @").unwrap().last().unwrap().kind, "guard");
}
#[test]
fn ambiguous_slashes_remain_explicit_and_unambiguous_division_is_preserved() {
    for id in ["typescript", "tsx", "jsx"] {
        let error = analyze_text(
            id,
            "const value = a / b / c;",
            &format!("ambiguous-slash-{id}"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("ambiguous token"));
        let r = analyze_text(
            id,
            "const value = a / b; const after = 2;",
            &format!("division-{id}"),
        )
        .unwrap();
        assert!(r.diagnostics.is_empty());
        assert!(r.symbols.symbols.iter().any(|s| s.name == "after"));
        let r = analyze_text(
            id,
            "const pattern = /[}/]/; const after = 2;",
            &format!("regex-class-{id}"),
        )
        .unwrap();
        assert!(r.diagnostics.is_empty());
    }
}
#[test]
fn root_comments_do_not_hide_declarations_after_declared_line_terminators() {
    for (i, end) in ["\n", "\r\n", "\r", "\u{2028}", "\u{2029}"]
        .iter()
        .enumerate()
    {
        let r = analyze_text(
            "typescript",
            &format!("// comment{end}const after = 2;"),
            &format!("root-line-{i}"),
        )
        .unwrap();
        assert!(
            r.symbols.symbols.iter().any(|s| s.name == "after"),
            "{end:?}"
        );
    }
}
#[test]
fn uncertain_property_and_postfix_contexts_do_not_absorb_real_neighboring_statements() {
    for (i, source) in [
        "const value = obj.return / item; const mustKeep = 1; /z/;",
        "const value = obj! / item; const mustKeep = 1; /z/;",
    ]
    .iter()
    .enumerate()
    {
        assert!(analyze_text("typescript", source, &format!("uncertain-slash-{i}")).is_err());
    }
}

#[test]
fn regex_after_prefix_negation_is_distinct_from_typescript_postfix_assertion() {
    for id in ["typescript", "tsx", "jsx"] {
        let result = analyze_text(
            id,
            "const valid = !/x[}]/.test(value); const after = 1;",
            &format!("negated-regexp-{id}"),
        )
        .unwrap();
        assert!(result.graph.nodes.iter().any(|n| n.name == "after"));
        assert!(result.tokens.iter().any(|t| t.kind == "regexp_literal"));
    }
    for id in ["typescript", "tsx"] {
        // The same bang spelling after an operand is a postfix assertion, not negation.
        assert!(
            analyze_text(
                id,
                "const result = value! / b / c;",
                &format!("postfix-regexp-guard-{id}")
            )
            .unwrap_err()
            .to_string()
            .contains("ambiguous token")
        );
    }
}
