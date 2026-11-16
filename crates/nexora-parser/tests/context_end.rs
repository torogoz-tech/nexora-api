use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use nexora_parser::Parser;
use std::path::Path;

#[test]
fn context_end_is_strictly_bounded_and_never_accepts_a_line_break_as_eof() {
    let mut language = LanguageDefinition::load(Path::new(
        "../../templates/composition-calls/region_expression",
    ))
    .unwrap();
    let mut grammar = serde_json::to_value(&language.grammar).unwrap();
    grammar["schema_version"] = 7.into();
    grammar["productions"].as_array_mut().unwrap().push(serde_json::json!({"name":"tail","pattern":{"type":"sequence","items":[{"type":"literal","text":"VAR"},{"type":"capture","name":"name","item":{"type":"token","kind":"identifier"}},{"type":"end_of_context"}]},"projection":{"ast_kind":"variable","role":"Declaration","name_capture":"name"}}));
    grammar["contexts"]["body"]["productions"] = serde_json::json!(["tail"]);
    language.grammar = serde_json::from_value(grammar).unwrap();
    language.validate().unwrap();
    let lexer = Lexer::new(&language.lexer).unwrap();
    let parser = Parser::new(language.grammar.clone());
    let tree = parser.parse(&lexer.tokenize("FUNC f() { VAR local } VAR after;").unwrap());
    assert!(tree.diagnostics.is_empty(), "{:?}", tree.diagnostics);
    assert!(tree.nodes.iter().any(|n| n.name == "local"));
    assert!(tree.nodes.iter().any(|n| n.name == "after"));
    for source in ["VAR value", "VAR value // trailing\n"] {
        let tokens = lexer.tokenize(source).unwrap();
        let parsed = parser
            .parse_in_context_measured(&tokens, "body", Default::default(), usize::MAX)
            .unwrap();
        assert!(parsed.tree.diagnostics.is_empty());
        assert!(parsed.tree.nodes.iter().any(|n| n.name == "value"));
        let limited = parser
            .parse_in_context_measured(&tokens, "body", Default::default(), parsed.steps - 1)
            .unwrap();
        assert!(limited.budget_exhausted);
    }
    let invalid = parser
        .parse_in_context(
            &lexer.tokenize("VAR value\n other").unwrap(),
            "body",
            Default::default(),
        )
        .unwrap();
    assert!(!invalid.diagnostics.is_empty());
    assert!(!invalid.nodes.iter().any(|n| n.name == "value"));
    language.grammar.schema_version = 6;
    assert!(language.validate().is_err());
}

#[test]
fn zero_width_guards_cannot_hide_left_recursion_from_validation() {
    for guard in [
        serde_json::json!({"type":"end_of_context"}),
        serde_json::json!({"type":"gap_without","characters":"!"}),
    ] {
        let mut language = LanguageDefinition::load(Path::new(
            "../../templates/composition-calls/region_expression",
        ))
        .unwrap();
        let mut grammar = serde_json::to_value(&language.grammar).unwrap();
        grammar["schema_version"] = 7.into();
        grammar["rules"]["recursive_tail"] = serde_json::json!({"type":"choice","alternatives":[{"type":"sequence","items":[guard,{"type":"rule","name":"recursive_tail"}]},{"type":"literal","text":";"}]});
        language.grammar = serde_json::from_value(grammar).unwrap();
        assert!(
            language.validate().is_err(),
            "left recursion behind a nullable guard must be rejected"
        );
    }
}
