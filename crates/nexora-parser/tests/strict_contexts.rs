use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use nexora_parser::Parser;
use std::path::Path;

#[test]
fn strict_context_stops_recovery_without_halting_sibling_or_parent_tasks() {
    let mut language = LanguageDefinition::load(Path::new(
        "../../templates/composition-calls/region_expression",
    ))
    .unwrap();
    let mut grammar = serde_json::to_value(&language.grammar).unwrap();
    grammar["schema_version"] = 6.into();
    grammar["contexts"]["parameters"]["stop_on_error"] = true.into();
    language.grammar = serde_json::from_value(grammar).unwrap();
    language.validate().unwrap();
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize("FUNC f(value 1\n forged) { VAR kept; } VAR after;")
        .unwrap();
    let tree = Parser::new(language.grammar.clone()).parse(&tokens);
    assert!(!tree.diagnostics.is_empty());
    assert!(!tree.nodes.iter().any(|n| n.name == "forged"));
    assert!(tree.nodes.iter().any(|n| n.name == "kept"));
    assert!(tree.nodes.iter().any(|n| n.name == "after"));
    let function = tree.nodes.iter().find(|n| n.name == "f").unwrap();
    assert!(
        function
            .regions
            .iter()
            .any(|r| r.capture == "parameters" && r.status == nexora_core::RegionStatus::Partial)
    );
    language.grammar.schema_version = 5;
    assert!(language.validate().is_err());
}
