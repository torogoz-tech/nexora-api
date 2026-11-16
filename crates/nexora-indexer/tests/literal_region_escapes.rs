use nexora_language::LexerDefinition;
use nexora_lexer::Lexer;

fn model(escapes: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "schema_version":7,"identifier_kind":"word","keywords":[],
        "tokens":[{"kind":"quote","pattern":"'","region":"quote"},{"kind":"word","pattern":"[a-z]+"},{"kind":"space","pattern":"\\s+","trivia":true}],
        "regions":{"quote":{"close":"'","escape":"\\","literal_escapes":escapes,"children":[{"pattern":"\\{","region":"child"}]},"child":{"close":"}"}}
    })
}

fn lexer(value: serde_json::Value) -> Lexer {
    let definition: LexerDefinition = serde_json::from_value(value).unwrap();
    Lexer::new(&definition).unwrap()
}

#[test]
fn complete_literal_escapes_consume_exactly_their_bytes_before_closers_and_children() {
    let lexer = lexer(model(serde_json::json!(["''", "{{", "}}", "éé"])));
    let source = "'a''b {{literal}} éé \\' {child}' tail";
    let tokens = lexer.tokenize(source).unwrap();
    assert_eq!(tokens[0].text, "'a''b {{literal}} éé \\' {child}'");
    assert_eq!(tokens[0].regions.len(), 2);
    assert!(tokens[0].regions.iter().all(|r| r.closed));
    assert_eq!(tokens.last().unwrap().text, "tail");
    assert_eq!(
        tokens[0].regions[1].opener.start.offset,
        source.find("{child}").unwrap()
    );
}

#[test]
fn longest_complete_escape_wins_independently_of_list_order() {
    for escapes in [
        serde_json::json!(["''", "'''x"]),
        serde_json::json!(["'''x", "''"]),
    ] {
        let tokens = lexer(model(escapes)).tokenize("''''xend' tail").unwrap();
        assert_eq!(tokens[0].text, "''''xend'");
        assert!(tokens[0].regions[0].closed);
    }
}

#[test]
fn bounded_ranges_never_complete_a_literal_using_bytes_outside_the_range() {
    let lexer = lexer(model(serde_json::json!(["''"])));
    let source = "xx 'a''b' tail";
    let tokens = lexer.tokenize_range(source, 3..6).unwrap();
    assert_eq!(tokens[0].text, "'a'");
    assert!(tokens[0].regions[0].closed);
    assert_eq!(tokens[0].regions[0].span.end.offset, 6);
    let tokens = lexer.tokenize("'a''").unwrap();
    assert!(!tokens[0].regions[0].closed);
}

#[test]
fn literal_escapes_are_body_only_and_do_not_override_header_endings() {
    let mut value = model(serde_json::json!(["''", "##"]));
    value["regions"]["quote"]["header"] = serde_json::json!({"end":"#"});
    let tokens = lexer(value).tokenize("'##text' tail").unwrap();
    let region = &tokens[0].regions[0];
    assert!(region.closed && region.header_closed);
    assert_eq!(region.content_span.start.offset, 2);
}

#[test]
fn literal_escape_configuration_and_work_are_bounded_and_versioned() {
    for escapes in [
        serde_json::json!([""]),
        serde_json::json!(["xx", "xx"]),
        serde_json::json!(["x".repeat(129)]),
        serde_json::json!(["é".repeat(65)]),
        serde_json::json!((0..17).map(|i| format!("e{i}")).collect::<Vec<_>>()),
    ] {
        let definition: LexerDefinition = serde_json::from_value(model(escapes)).unwrap();
        assert!(Lexer::new(&definition).is_err());
    }
    let mut value = model(serde_json::json!(["''"]));
    value["schema_version"] = 6.into();
    let definition: LexerDefinition = serde_json::from_value(value).unwrap();
    assert!(Lexer::new(&definition).is_err());
    let mut value = model(serde_json::json!(["''"]));
    value["max_region_steps"] = 3.into();
    assert!(lexer(value).tokenize("'a'''''''''''''''''").is_err());
}

#[test]
fn literal_escape_precedes_prefix_escape_and_preserves_unicode_positions() {
    let lexer = lexer(model(serde_json::json!(["\\é'"])));
    let tokens = lexer.tokenize("'\\é'' tail").unwrap();
    assert_eq!(tokens[0].text, "'\\é''");
    assert!(tokens[0].regions[0].closed);
    assert_eq!(tokens[0].span.end.offset, 6);
    assert_eq!(tokens[0].span.end.column, 6);
}

#[test]
fn omitted_literal_escapes_preserve_schema_six_prefix_escape_behavior() {
    let mut value = model(serde_json::json!([]));
    value["schema_version"] = 6.into();
    value["regions"]["quote"]
        .as_object_mut()
        .unwrap()
        .remove("literal_escapes");
    let tokens = lexer(value).tokenize("'a\\'b' tail").unwrap();
    assert_eq!(tokens[0].text, "'a\\'b'");
    assert!(tokens[0].regions[0].closed);
}
