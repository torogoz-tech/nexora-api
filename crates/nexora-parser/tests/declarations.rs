use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use nexora_parser::Parser;
use std::path::Path;
#[test]
fn incomplete_and_anonymous_declarations_do_not_become_nodes() {
    let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    let lexer = Lexer::new(&language.lexer).unwrap();
    let parser = Parser::new(language.grammar.clone());
    for source in [
        "function x() {",
        "class {}",
        "function x { }",
        "function x(] {}",
    ] {
        let tree = parser.parse(&lexer.tokenize(source).unwrap());
        assert!(!tree.diagnostics.is_empty(), "{source}");
        assert!(tree.nodes.is_empty(), "{source}");
    }
}
#[test]
fn nested_groups_and_strings_do_not_create_spurious_declarations() {
    let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize("function x(a) { if(a) { return '}'; } function hidden() {} } class Y {}")
        .unwrap();
    let tree = Parser::new(language.grammar).parse(&tokens);
    assert!(tree.diagnostics.is_empty(), "{:?}", tree.diagnostics);
    assert_eq!(
        tree.nodes
            .iter()
            .map(|n| n.name.as_str())
            .collect::<Vec<_>>(),
        ["x", "Y"]
    );
}
