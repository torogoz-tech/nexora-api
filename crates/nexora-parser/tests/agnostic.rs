use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use nexora_parser::Parser;
use std::path::Path;
#[test]
fn lua_end_blocks_and_module_bindings_use_the_common_interpreter() {
    let language = LanguageDefinition::load(Path::new("../../languages/lua")).unwrap();
    let source = "local storage = require(\"infrastructure.storage\")\nfunction total(order)\n if order then return storage.save(order) end\nend\n";
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize(source)
        .unwrap();
    let tree = Parser::new(language.grammar).parse(&tokens);
    assert!(tree.diagnostics.is_empty(), "{:?}", tree.diagnostics);
    assert_eq!(
        tree.nodes
            .iter()
            .map(|n| n.name.as_str())
            .collect::<Vec<_>>(),
        ["\"infrastructure.storage\"", "total"]
    );
}
