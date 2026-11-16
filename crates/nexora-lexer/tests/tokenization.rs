use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use std::path::Path;
fn lexer() -> Lexer {
    Lexer::new(
        &LanguageDefinition::load(Path::new("../../languages/javascript"))
            .unwrap()
            .lexer,
    )
    .unwrap()
}
#[test]
fn lossless_unicode_crlf_and_longest_match() {
    let source = "// hi\r\nfunction café() { return \"hé\\\"llo\" === 42; }";
    let tokens = lexer().tokenize(source).unwrap();
    assert_eq!(
        tokens.iter().map(|t| t.text.as_str()).collect::<String>(),
        source
    );
    for token in &tokens {
        assert_eq!(
            &source[token.span.start.offset..token.span.end.offset],
            token.text
        );
    }
    let function = tokens.iter().find(|t| t.text == "function").unwrap();
    assert_eq!(
        (function.span.start.line, function.span.start.column),
        (2, 1)
    );
    assert!(tokens.iter().any(|t| t.text == "==="));
    assert!(
        tokens
            .iter()
            .any(|t| t.text == "café" && t.kind == "identifier")
    );
}
#[test]
fn bad_literals_and_unknown_tokens_fail() {
    for source in ["'unterminated", "/* unterminated", "💥"] {
        assert!(lexer().tokenize(source).is_err(), "{source}");
    }
}

#[test]
fn comment_syntax_is_defined_only_in_yaml() {
    let mut language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    language
        .lexer
        .tokens
        .retain(|r| !r.kind.contains("comment"));
    // A language without comment rules must treat slash and star as operators.
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize("/* text")
        .unwrap();
    assert_eq!(tokens[0].text, "/");
    assert_eq!(tokens[1].text, "*");
}

#[test]
fn large_source_ranges_are_not_rejected_by_total_file_size() {
    let mut source = " ".repeat(5 * 1024 * 1024);
    source.push_str("class Large {}");
    let tokens = lexer()
        .tokenize_range(&source, source.len() - 14..source.len())
        .unwrap();
    assert!(tokens.iter().any(|token| token.text == "Large"));
}
