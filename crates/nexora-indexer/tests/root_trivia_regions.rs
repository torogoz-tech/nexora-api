use nexora_language::{GrammarDefinition, LexerDefinition};
use nexora_lexer::Lexer;
use nexora_parser::Parser;
fn definition(version: u32) -> LexerDefinition {
    serde_json::from_value(serde_json::json!({"schema_version":version,"identifier_kind":"word","keywords":[],"tokens":[{"kind":"note","pattern":"\\[","region":"note","trivia":true},{"kind":"word","pattern":"[a-z]+"},{"kind":"suffix","pattern":"!","after_kinds":["word"],"at_start":false,"reject_on_context_mismatch":true},{"kind":"space","pattern":"\\s+","trivia":true}],"regions":{"note":{"close":"]","children":[{"pattern":"\"","region":"quote"}]},"quote":{"close":"\""}}})).unwrap()
}
#[test]
fn trivia_region_preserves_previous_significant_kind_and_unicode_delimiter_evidence() {
    let source = "alpha [é \"]\" ] !";
    let tokens = Lexer::new(&definition(11))
        .unwrap()
        .tokenize(source)
        .unwrap();
    assert_eq!(
        tokens
            .iter()
            .filter(|t| !t.trivia)
            .map(|t| t.kind.as_str())
            .collect::<Vec<_>>(),
        ["word", "suffix"]
    );
    let note = tokens.iter().find(|t| t.kind == "note").unwrap();
    assert!(note.trivia);
    assert_eq!(note.regions.len(), 2);
    assert!(note.regions.iter().all(|r| r.closed));
    let region = &note.regions[0];
    assert_eq!(
        &source[region.span.start.offset..region.span.end.offset],
        "[é \"]\" ]"
    );
    assert_eq!(region.closer.unwrap().end, region.span.end);
    assert_eq!(note.regions[1].parent.as_ref(), Some(&region.id));
    assert_eq!(note.span.end.column, 15);
}
#[test]
fn old_schemas_reject_region_trivia_and_rejected_regions_remain_invalid() {
    for version in [1, 5, 10] {
        assert!(Lexer::new(&definition(version)).is_err());
    }
    let mut d = definition(11);
    d.tokens[0].reject = true;
    assert!(Lexer::new(&d).is_err());
    d.tokens[0].reject = false;
    d.tokens[0].trivia = false;
    d.schema_version = 10;
    assert!(Lexer::new(&d).is_ok());
}
#[test]
fn parser_projects_trivia_regions_and_reports_truncation_without_parsing_their_text() {
    let grammar: GrammarDefinition = serde_json::from_value(serde_json::json!({"schema_version":3,"max_nesting":32,"max_steps":10000,"root_productions":["item"],"productions":[{"name":"item","pattern":{"type":"capture","name":"name","item":{"type":"token","kind":"word"}},"projection":{"ast_kind":"word","role":"Container","name_capture":"name"}}]})).unwrap();
    let lexer = Lexer::new(&definition(11)).unwrap();
    for (source, closed) in [
        ("alpha [fake declaration]", true),
        ("alpha [fake declaration", false),
    ] {
        let tokens = lexer.tokenize(source).unwrap();
        let cst = Parser::new(grammar.clone()).parse(&tokens);
        assert_eq!(
            cst.nodes.iter().filter(|n| n.production == "item").count(),
            1
        );
        let regions: Vec<_> = cst
            .nodes
            .iter()
            .filter_map(|n| n.lexical_region.as_ref())
            .collect();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].closed, closed);
        assert_eq!(cst.diagnostics.len(), usize::from(!closed));
    }
}
