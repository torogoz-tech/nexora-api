use nexora_core::SyntaxRole;
use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use nexora_parser::{ConcreteSyntaxTree, Parser};
use std::path::Path;
fn language() -> LanguageDefinition {
    LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap()
}
fn parse(source: &str) -> ConcreteSyntaxTree {
    let language = language();
    Parser::new(language.grammar).parse(
        &Lexer::new(&language.lexer)
            .unwrap()
            .tokenize(source)
            .unwrap(),
    )
}
#[test]
fn static_forms_preserve_source_ranges_and_reference_role() {
    for source in [
        "import './x.js';",
        "import X from './x.js';",
        "import * as X from './x.js';",
        "import {x, y as z,} from './x.js';",
        "import X, {y} from './x.js';",
        "import X, * as Y from './x.js';",
        "import './x.js'\nclass A {}",
    ] {
        let tree = parse(source);
        assert!(
            tree.diagnostics.is_empty(),
            "{source}: {:?}",
            tree.diagnostics
        );
        let imports: Vec<_> = tree
            .nodes
            .iter()
            .filter(|n| n.role == SyntaxRole::Reference)
            .collect();
        assert_eq!(imports.len(), 1, "{source}");
        assert_eq!(imports[0].name, "'./x.js'");
        assert!(
            source[imports[0].span.start.offset..imports[0].span.end.offset].starts_with("import")
        );
    }
}
#[test]
fn malformed_imports_do_not_become_references() {
    for source in [
        "import X './x.js';",
        "import {x y} from './x.js';",
        "import * X from './x.js';",
        "import {x as} from './x.js';",
        "import './x.js' class A {}",
        "import('./x.js');",
    ] {
        let tree = parse(source);
        assert!(!tree.diagnostics.is_empty(), "{source}");
        assert!(
            !tree.nodes.iter().any(|n| n.role == SyntaxRole::Reference),
            "{source}"
        );
    }
}
#[test]
fn opaque_loaders_are_diagnosed_and_strings_and_comments_are_not_imports() {
    let tree = parse("function x() { import('./x.js'); } class A {}");
    assert!(!tree.diagnostics.is_empty());
    let tree = parse("function x() { require('./x.js'); } ");
    assert!(!tree.diagnostics.is_empty());
    let tree = parse("// import './x.js';\nfunction x() { return \"import './x.js';\"; }");
    assert!(tree.diagnostics.is_empty());
    assert!(tree.nodes.iter().all(|n| n.role == SyntaxRole::Declaration));
}
#[test]
fn module_reference_keywords_are_declarative() {
    let mut language = language();
    let grammar = serde_yaml_ng::to_string(&language.grammar)
        .unwrap()
        .replace("text: import", "text: usemodule")
        .replace("text: from", "text: via")
        .replace("text: as", "text: alias");
    language.grammar = serde_yaml_ng::from_str(&grammar).unwrap();
    language
        .lexer
        .keywords
        .extend(["usemodule".into(), "via".into(), "alias".into()]);
    let source = "usemodule {x alias y} via './x.js';";
    let tree = Parser::new(language.grammar).parse(
        &Lexer::new(&language.lexer)
            .unwrap()
            .tokenize(source)
            .unwrap(),
    );
    assert!(tree.diagnostics.is_empty());
    assert_eq!(tree.nodes[0].role, SyntaxRole::Reference);
}

#[test]
fn contextual_words_remain_valid_declaration_and_import_binding_names() {
    for source in [
        "function from() {} class as {}",
        "import from from './x.js';",
        "import {from as as, as as from} from './x.js';",
    ] {
        let tree = parse(source);
        assert!(
            tree.diagnostics.is_empty(),
            "{source}: {:?}",
            tree.diagnostics
        );
        assert!(!tree.nodes.is_empty());
    }
}
#[test]
fn parenthesized_or_aliased_loaders_in_opaque_bodies_are_not_certified_complete() {
    for source in [
        "function load() { (require)('../x.js'); }",
        "function load() { const loader = require; loader('../x.js'); }",
    ] {
        let tree = parse(source);
        assert!(!tree.diagnostics.is_empty(), "{source}");
    }
}
