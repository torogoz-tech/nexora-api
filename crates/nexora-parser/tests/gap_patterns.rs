use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use nexora_parser::Parser;
use std::path::Path;

#[test]
fn declarative_gap_constraint_reads_trivia_and_respects_context_start_and_budget() {
    let mut language = LanguageDefinition::load(Path::new(
        "../../templates/composition-calls/region_expression",
    ))
    .unwrap();
    let mut grammar = serde_json::to_value(&language.grammar).unwrap();
    grammar["schema_version"] = 6.into();
    for production in grammar["productions"].as_array_mut().unwrap() {
        if production["name"] == "function" {
            production["pattern"]["items"]
                .as_array_mut()
                .unwrap()
                .insert(
                    1,
                    serde_json::json!({"type":"gap_without","characters":"!"}),
                );
        }
        if production["name"] == "parameter" {
            let original = production["pattern"].clone();
            production["pattern"] = serde_json::json!({"type":"sequence","items":[{"type":"gap_without","characters":"!"},original]});
        }
    }
    language.grammar = serde_json::from_value(grammar).unwrap();
    language.validate().unwrap();
    let lexer = Lexer::new(&language.lexer).unwrap();
    let parser = Parser::new(language.grammar.clone());
    let blocked = parser.parse(&lexer.tokenize("FUNC //!\n f() {}").unwrap());
    assert!(!blocked.diagnostics.is_empty());
    assert!(!blocked.nodes.iter().any(|n| n.name == "f"));
    let tokens = lexer.tokenize("FUNC // okay\n f(//!\n value) {}").unwrap();
    let valid = parser.parse_measured(&tokens, usize::MAX);
    assert!(
        valid.tree.diagnostics.is_empty(),
        "{:?}",
        valid.tree.diagnostics
    );
    assert!(valid.tree.nodes.iter().any(|n| n.name == "value"));
    let limited = parser.parse_measured(&tokens, valid.steps - 1);
    assert!(limited.budget_exhausted);
    language.grammar.schema_version = 5;
    assert!(language.validate().is_err());
}
