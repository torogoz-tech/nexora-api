use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use nexora_parser::Parser;
use std::path::Path;

#[test]
fn a_child_production_cannot_capture_a_declaration_after_its_context_closer() {
    let mut language = LanguageDefinition::load(Path::new(
        "../../templates/composition-calls/region_expression",
    ))
    .unwrap();
    language
        .grammar
        .contexts
        .get_mut("body")
        .unwrap()
        .productions = vec!["steal".into()];
    language.grammar.productions.push(
        serde_json::from_value(serde_json::json!({
            "name":"steal", "pattern":{"type":"sequence","items":[
                {"type":"literal","text":"TAKE"},{"type":"literal","text":"}"},
                {"type":"literal","text":"VAR"},
                {"type":"capture","name":"name","item":{"type":"token","kind":"identifier"}},
                {"type":"literal","text":";"}
            ]},"projection":{"ast_kind":"variable","role":"Declaration","name_capture":"name"}
        }))
        .unwrap(),
    );
    language.validate().unwrap();
    let source = "FUNC f() { TAKE } VAR outside;";
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize(source)
        .unwrap();
    let tree = Parser::new(language.grammar).parse(&tokens);
    assert_eq!(tree.nodes.iter().filter(|n| n.name == "outside").count(), 1);
    assert!(!tree.diagnostics.is_empty());
    let function = tree.nodes.iter().find(|n| n.name == "f").unwrap();
    for node in tree
        .nodes
        .iter()
        .filter(|n| n.parent_id.as_ref() == Some(&function.id))
    {
        assert!(node.span.end.offset <= function.span.end.offset);
    }
}

fn grammar_five() -> LanguageDefinition {
    let mut language = LanguageDefinition::load(Path::new(
        "../../templates/composition-calls/region_expression",
    ))
    .unwrap();
    let mut grammar = serde_json::to_value(&language.grammar).unwrap();
    grammar["schema_version"] = 5.into();
    grammar["contexts"]["expression"]["expression"]["literal_selectors"] =
        serde_json::json!([{"text":"ON","kind":"keyword"},{"text":"NIL","kind":"identifier"}]);
    language.lexer.keywords.push("ON".into());
    language.grammar = serde_json::from_value(grammar).unwrap();
    language
}

#[test]
fn literal_selectors_distinguish_declared_values_from_other_keywords_and_names() {
    let language = grammar_five();
    language.validate().unwrap();
    for (source, kind) in [
        ("ON", nexora_core::ExpressionKind::Literal),
        ("NIL", nexora_core::ExpressionKind::Literal),
        ("other", nexora_core::ExpressionKind::Identifier),
    ] {
        let tokens = Lexer::new(&language.lexer)
            .unwrap()
            .tokenize(source)
            .unwrap();
        let tree = Parser::new(language.grammar.clone())
            .parse_in_context(&tokens, "expression", Default::default())
            .unwrap();
        assert!(tree.diagnostics.is_empty());
        assert_eq!(
            tree.nodes
                .iter()
                .find_map(|n| n.expression.as_ref())
                .unwrap()
                .kind,
            kind
        );
    }
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize("FUNC")
        .unwrap();
    let tree = Parser::new(language.grammar)
        .parse_in_context(&tokens, "expression", Default::default())
        .unwrap();
    assert!(!tree.diagnostics.is_empty());
}

#[test]
fn eof_scanning_stops_at_the_active_body_boundary_and_requires_nonempty_input() {
    let mut language = grammar_five();
    language
        .grammar
        .contexts
        .get_mut("body")
        .unwrap()
        .productions = vec!["tail".into()];
    language.grammar.productions.push(serde_json::from_value(serde_json::json!({
        "name":"tail","pattern":{"type":"sequence","items":[
            {"type":"capture","name":"name","item":{"type":"token","kind":"identifier"}},
            {"type":"literal","text":"="},
            {"type":"capture","name":"value","item":{"type":"scan_until","stop":[],"allow_eof":true}}
        ]},"projection":{"ast_kind":"statement","role":"Container","name_capture":"name","regions":[{"capture":"value"}]}
    })).unwrap());
    language.validate().unwrap();
    let source = "FUNC f() { body = payload } VAR outside;";
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize(source)
        .unwrap();
    let tree = Parser::new(language.grammar.clone()).parse(&tokens);
    assert!(tree.diagnostics.is_empty(), "{:?}", tree.diagnostics);
    let tail = tree.nodes.iter().find(|n| n.name == "body").unwrap();
    let value = tail.captures.get("value").unwrap();
    assert_eq!(
        &source[value.span.start.offset..value.span.end.offset],
        "payload"
    );
    assert_eq!(tree.nodes.iter().filter(|n| n.name == "outside").count(), 1);
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize("FUNC f() { body = } VAR outside;")
        .unwrap();
    let tree = Parser::new(language.grammar).parse(&tokens);
    assert!(!tree.diagnostics.is_empty());
    assert_eq!(tree.nodes.iter().filter(|n| n.name == "outside").count(), 1);
}

#[test]
fn grammar_five_options_are_gated_and_literal_operator_conflicts_are_rejected() {
    let mut language = grammar_five();
    language.grammar.schema_version = 4;
    assert!(language.validate().is_err());
    let mut language = grammar_five();
    let mut grammar = serde_json::to_value(&language.grammar).unwrap();
    grammar["contexts"]["expression"]["expression"]["literal_selectors"] =
        serde_json::json!([{"text":"-","kind":"operator"}]);
    language.grammar = serde_json::from_value(grammar).unwrap();
    assert!(language.validate().is_err());
}

#[test]
fn literal_keywords_still_work_as_member_names_and_scan_budgets_are_shared() {
    use nexora_language::GrammarPattern;
    use nexora_parser::PatternMatcher;
    use std::collections::BTreeMap;
    let mut language = grammar_five();
    language.lexer.schema_version = 5;
    language.lexer.keyword_after_exclusions =
        vec![serde_json::from_value(serde_json::json!({"text":"."})).unwrap()];
    let source = "object.ON";
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize(source)
        .unwrap();
    let tree = Parser::new(language.grammar.clone())
        .parse_in_context(&tokens, "expression", Default::default())
        .unwrap();
    assert!(tree.diagnostics.is_empty());
    assert!(tree.nodes.iter().any(|n| {
        n.name == "ON"
            && n.expression
                .as_ref()
                .is_some_and(|s| s.kind == nexora_core::ExpressionKind::Identifier)
    }));
    language.grammar.max_steps = 10;
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize(&"value ".repeat(30))
        .unwrap();
    let significant: Vec<_> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.trivia)
        .collect();
    let mut matcher = PatternMatcher::new(&language.grammar, &significant);
    let scan: GrammarPattern =
        serde_json::from_value(serde_json::json!({"type":"scan_until","stop":[],"allow_eof":true}))
            .unwrap();
    let error = matcher
        .pattern(&scan, 0, &mut BTreeMap::new(), 0)
        .unwrap_err();
    assert!(error.fatal);
}
