use nexora_core::SourcePositionIndex;
use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use std::path::Path;

#[test]
fn indexed_ranges_keep_original_unicode_positions_and_cannot_read_past_the_end() {
    let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    let lexer = Lexer::new(&language.lexer).unwrap();
    let source = "// é🦀\r\nvalue + 1; outside";
    let index = SourcePositionIndex::new(source).unwrap();
    let tokens = lexer.tokenize_indexed_range(&index, 11..20).unwrap();
    let value = tokens.iter().find(|t| t.text == "value").unwrap();
    assert_eq!(
        (
            value.span.start.offset,
            value.span.start.line,
            value.span.start.column
        ),
        (11, 2, 1)
    );
    assert!(!tokens.iter().any(|t| t.text == "outside"));
    assert!(tokens.iter().all(|t| t.span.end.offset <= 20));
    assert!(lexer.tokenize_indexed_range(&index, 4..5).is_err());
    assert!(lexer.tokenize_indexed_range(&index, 20..99).is_err());
    let start = 20;
    let end = 11;
    assert!(lexer.tokenize_indexed_range(&index, start..end).is_err());
}
