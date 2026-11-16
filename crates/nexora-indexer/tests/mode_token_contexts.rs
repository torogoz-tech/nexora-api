use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use std::path::Path;
fn definition() -> nexora_language::LexerDefinition {
    let mut value = serde_json::to_value(
        LanguageDefinition::load(Path::new("../../languages/tsx"))
            .unwrap()
            .lexer,
    )
    .unwrap();
    value["schema_version"] = 5.into();
    value["keyword_after_exclusions"] = serde_json::json!([{"text":"."},{"text":"?."}]);
    let kinds: Vec<_> = value["tokens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["kind"].clone())
        .collect();
    value["regions"]["code"]["token_rules"] = serde_json::json!(kinds);
    value["regions"]["code"]["children"]
        .as_array_mut()
        .unwrap()
        .retain(|t| t["reject"] != true);
    for (mode, kind, trivia) in [
        ("template", "template", false),
        ("double_quote", "string", false),
        ("single_quote", "string", false),
        ("line_comment", "line_comment", true),
        ("block_comment", "block_comment", true),
    ] {
        value["regions"][mode]["token_kind"] = kind.into();
        value["regions"][mode]["trivia"] = trivia.into();
    }
    for token in value["tokens"].as_array_mut().unwrap() {
        if token["kind"] == "jsx_atom" || token["kind"] == "jsx_fragment" {
            for selector in token["after"].as_array_mut().unwrap() {
                if ["return", "throw", "yield"].contains(&selector["text"].as_str().unwrap()) {
                    selector["kind"] = "keyword".into();
                }
            }
        }
    }
    serde_json::from_value(value).unwrap()
}
#[test]
fn nested_jsx_in_code_is_a_region_and_preserves_neighbors_and_source() {
    let source = "const View = <Outer>{items.map((item) => <Inner title={item.name}>const fake = 1;</Inner>)}</Outer>; const after = 2;";
    let tokens = Lexer::new(&definition()).unwrap().tokenize(source).unwrap();
    assert_eq!(
        tokens.iter().map(|t| t.text.as_str()).collect::<String>(),
        source
    );
    let jsx = tokens.iter().find(|t| t.kind == "jsx_atom").unwrap();
    assert_eq!(
        jsx.regions.iter().filter(|r| r.mode == "element").count(),
        2
    );
    assert!(jsx.regions.iter().all(|r| r.closed));
    assert!(tokens.iter().any(|t| t.text == "after"));
    assert!(!tokens.iter().any(|t| t.text == "fake"));
}
#[test]
fn comments_and_templates_do_not_erase_significant_context() {
    for source in [
        "const v = <Box>{ok && /* > } */ <Inner />}</Box>;",
        "const v = `text ${ok ? <><Inner /></> : null}`;",
        "const v = <Box>{() => { return <Inner />; }}</Box>;",
        "const v = <Box>{\"text\" + <Inner />}</Box>;",
        "const v = <Box>{function* f(){ yield <Inner />; }}</Box>;",
        "const v = <Box>{() => { throw <Inner />; }}</Box>;",
    ] {
        let tokens = Lexer::new(&definition()).unwrap().tokenize(source).unwrap();
        assert_eq!(
            tokens.iter().map(|t| t.text.as_str()).collect::<String>(),
            source
        );
        assert!(tokens.iter().flat_map(|t| &t.regions).all(|r| r.closed));
        assert!(
            tokens
                .iter()
                .flat_map(|t| &t.regions)
                .any(|r| r.mode == "element"
                    && source[r.span.start.offset..r.span.end.offset].starts_with("<Inner"))
        );
    }
}
#[test]
fn keyword_members_and_generic_arrows_are_not_treated_as_jsx_openings() {
    for source in [
        "const v = <Box>{obj.return<T>()}</Box>;",
        "const v = <Box>{obj.yield<T>() + obj.throw<T>()}</Box>;",
        "const v = <Box>{<T extends unknown>(value: T) => value}</Box>;",
        "const v = <Box>{<T = unknown>(value: T) => value}</Box>;",
        "const v = <Box>{<T,>(value: T) => value}</Box>;",
    ] {
        let tokens = Lexer::new(&definition()).unwrap().tokenize(source).unwrap();
        assert_eq!(
            tokens
                .iter()
                .flat_map(|t| &t.regions)
                .filter(|r| r.mode == "element")
                .count(),
            1
        );
        assert!(tokens.iter().flat_map(|t| &t.regions).all(|r| r.closed));
    }
}
#[test]
fn regex_closing_braces_are_masked_and_ambiguous_slashes_stay_explicit() {
    let lexer = Lexer::new(&definition()).unwrap();
    let source = "const v = <Box>{/[/}]/.test(text) ? <Inner /> : null}</Box>; const after = 1;";
    let tokens = lexer.tokenize(source).unwrap();
    assert!(tokens.iter().any(|t| t.text == "after"));
    assert!(tokens.iter().flat_map(|t| &t.regions).all(|r| r.closed));
    assert!(lexer.tokenize("const v=<Box>{a / b / c}</Box>;").is_err());
    assert!(lexer.tokenize("const v=`${a / b}`;").is_ok());
    // A later closing-tag slash still forms an ambiguous literal candidate.
    // Keep this conservative limitation explicit until lexical goals can disambiguate it.
    assert!(
        lexer
            .tokenize("const v=<Box>{a / b}</Box>;")
            .unwrap_err()
            .to_string()
            .contains("ambiguous token")
    );
}
#[test]
fn mode_token_configuration_and_budgets_are_validated() {
    let mut d = definition();
    d.schema_version = 4;
    assert!(Lexer::new(&d).is_err());
    let mut d = definition();
    d.regions
        .get_mut("code")
        .unwrap()
        .token_rules
        .push("missing".into());
    assert!(Lexer::new(&d).is_err());
    let mut d = definition();
    d.max_region_depth = 3;
    assert!(
        Lexer::new(&d)
            .unwrap()
            .tokenize("const v=<A>{ok && <B>{<C/>}</B>}</A>;")
            .is_err()
    );
}

#[test]
fn generic_disambiguation_applies_to_code_but_not_literal_markup_children() {
    let source = "const v=<Box><T extends unknown>(x) => text</T></Box>; const after=1;";
    let tokens = Lexer::new(&definition()).unwrap().tokenize(source).unwrap();
    assert!(tokens.iter().any(|t| t.text == "after"));
    assert_eq!(
        tokens
            .iter()
            .flat_map(|t| &t.regions)
            .filter(|r| r.mode == "element")
            .count(),
        2
    );
    assert!(tokens.iter().flat_map(|t| &t.regions).all(|r| r.closed));
}

#[test]
fn follow_exclusions_are_generic_token_guards_and_require_schema_five() {
    let definition: nexora_language::LexerDefinition = serde_json::from_value(serde_json::json!({
        "schema_version":5,"identifier_kind":"word","keywords":[],
        "tokens":[
            {"kind":"guarded","pattern":"@","follow_exclude":"END"},
            {"kind":"fallback","pattern":"@"},
            {"kind":"word","pattern":"[A-Z]+"}
        ]
    }))
    .unwrap();
    let lexer = Lexer::new(&definition).unwrap();
    assert_eq!(lexer.tokenize("@END").unwrap()[0].kind, "fallback");
    assert_eq!(lexer.tokenize("@OK").unwrap()[0].kind, "guarded");
    let mut legacy = definition.clone();
    legacy.schema_version = 4;
    assert!(Lexer::new(&legacy).is_err());
    let mut malformed = definition.clone();
    malformed.tokens[0].follow_exclude = Some("[".into());
    assert!(Lexer::new(&malformed).is_err());
    let mut excessive = definition;
    excessive.tokens[0].follow_exclude = Some("a".repeat(4097));
    assert!(Lexer::new(&excessive).is_err());
}
