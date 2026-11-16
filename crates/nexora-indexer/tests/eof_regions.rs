use nexora_language::LexerDefinition;
use nexora_lexer::Lexer;

fn definition() -> LexerDefinition {
    serde_json::from_value(serde_json::json!({
        "schema_version":6,"identifier_kind":"word","keywords":[],
        "tokens":[{"kind":"tail","pattern":">>>","region":"tail"},{"kind":"space","pattern":"\\s+","trivia":true},{"kind":"word","pattern":"[A-Za-z]+"}],
        "regions":{"tail":{"close":"","eof_closes":true},"quote":{"close":"\""}}
    })).unwrap()
}

#[test]
fn eof_only_region_has_no_sentinel_or_fabricated_closer_even_for_empty_tail() {
    let lexer = Lexer::new(&definition()).unwrap();
    for source in [">>>", ">>> literal <<< >>> </tail> \0 é"] {
        let tokens = lexer.tokenize(source).unwrap();
        assert_eq!(tokens.len(), 1);
        let region = &tokens[0].regions[0];
        assert!(region.closed);
        assert!(region.closer.is_none());
        assert_eq!(region.content_span.start.offset, 3);
        assert_eq!(region.content_span.end.offset, source.len());
        assert_eq!(tokens[0].text, source);
    }
}

#[test]
fn eof_only_region_uses_the_bounded_input_end_and_original_positions() {
    let lexer = Lexer::new(&definition()).unwrap();
    let tokens = lexer
        .tokenize_range("skip >>> text END suffix", 5..14)
        .unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].text, ">>> text ");
    let region = &tokens[0].regions[0];
    assert_eq!(region.span.start.offset, 5);
    assert_eq!(region.content_span.start.offset, 8);
    assert_eq!(region.content_span.end.offset, 14);
    assert!(region.closed && region.closer.is_none());
}

#[test]
fn eof_only_configuration_is_versioned_and_cannot_hide_unclosed_children_or_cost() {
    let mut d = definition();
    d.schema_version = 5;
    assert!(Lexer::new(&d).is_err());
    for change in [
        serde_json::json!({"close":"","eof_closes":false}),
        serde_json::json!({"close":"","eof_closes":true,"alternate_closes":["END"]}),
        serde_json::json!({"close":"","eof_closes":true,"close_capture":"name"}),
        serde_json::json!({"close":"","eof_closes":true,"close_tail":"END"}),
    ] {
        let mut json = serde_json::to_value(definition()).unwrap();
        json["regions"]["tail"] = change;
        let invalid = serde_json::from_value(json).unwrap();
        assert!(Lexer::new(&invalid).is_err());
    }
    let mut json = serde_json::to_value(definition()).unwrap();
    json["regions"]["tail"]["children"] = serde_json::json!([{"pattern":"\"","region":"quote"}]);
    let children = serde_json::from_value(json).unwrap();
    let tokens = Lexer::new(&children)
        .unwrap()
        .tokenize(">>> \"unfinished")
        .unwrap();
    assert!(tokens[0].regions.iter().all(|r| !r.closed));
    let mut limited = definition();
    limited.max_region_steps = 3;
    assert!(
        Lexer::new(&limited)
            .unwrap()
            .tokenize(">>> lots of literal data")
            .is_err()
    );
}
