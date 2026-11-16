use nexora_language::LexerDefinition;
use nexora_lexer::Lexer;
fn definition() -> serde_json::Value {
    serde_json::json!({"schema_version":9,"identifier_kind":"word","keywords":[],"tokens":[{"kind":"implicit","pattern":"@","region":"implicit"},{"kind":"word","pattern":"[a-z]+"},{"kind":"punctuation","pattern":"[;{}\\s]"}],"regions":{"implicit":{"close":"","close_before":[{"pattern":"[;}]"}],"children":[{"pattern":"\\{","region":"nested"}]},"nested":{"close":"}"}}})
}
fn lexer(value: serde_json::Value) -> Lexer {
    Lexer::new(&serde_json::from_value(value).unwrap()).unwrap()
}
#[test]
fn boundary_match_is_evidence_but_is_not_consumed_or_fabricated_as_a_closer() {
    let source = "@body;tail";
    let tokens = lexer(definition()).tokenize(source).unwrap();
    assert_eq!(
        tokens.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(),
        ["@body", ";", "tail"]
    );
    let region = &tokens[0].regions[0];
    assert!(region.closed);
    assert!(region.closer.is_none());
    let json = serde_json::to_value(region).unwrap();
    assert_eq!(json["lookahead_span"]["start"]["offset"], 5);
    assert_eq!(json["lookahead_span"]["end"]["offset"], 6);
    assert_eq!(region.span.end.offset, 5);
}
#[test]
fn nested_children_and_parent_delimiters_keep_owning_their_characters() {
    let mut d = definition();
    d["tokens"][0]["pattern"] = "\\{".into();
    d["tokens"][0]["region"] = "outer".into();
    d["tokens"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"kind":"entry","pattern":"@","region":"implicit"}));
    d["regions"]["outer"] =
        serde_json::json!({"close":"}","children":[{"pattern":"@","region":"implicit"}]});
    let tokens = lexer(d).tokenize("{@word{nested} } tail").unwrap();
    let regions = &tokens[0].regions;
    assert!(regions.iter().all(|r| r.closed));
    assert_eq!(tokens[0].text, "{@word{nested} }");
    assert_eq!(regions[1].content_span.end.offset, 15);
    assert_eq!(regions[0].closer.unwrap().start.offset, 15);
}
#[test]
fn bounded_ranges_and_eof_policy_do_not_read_outside_the_input() {
    let reader = lexer(definition());
    let tokens = reader.tokenize_range("xx @word; tail", 3..8).unwrap();
    assert_eq!(tokens[0].text, "@word");
    assert!(!tokens[0].regions[0].closed);
    let mut d = definition();
    d["regions"]["implicit"]["eof_closes"] = true.into();
    let tokens = lexer(d).tokenize("@word").unwrap();
    assert!(tokens[0].regions[0].closed);
    assert!(tokens[0].regions[0].closer.is_none());
}
#[test]
fn boundaries_require_finite_positive_patterns_valid_kinds_and_schema_nine() {
    for pattern in ["", "a*", "a+", ".{129}", "("] {
        let mut d = definition();
        d["regions"]["implicit"]["close_before"][0]["pattern"] = pattern.into();
        let definition: LexerDefinition = serde_json::from_value(d).unwrap();
        assert!(Lexer::new(&definition).is_err());
    }
    let mut d = definition();
    d["schema_version"] = 8.into();
    let definition: LexerDefinition = serde_json::from_value(d).unwrap();
    assert!(Lexer::new(&definition).is_err());
}

#[test]
fn boundary_kind_gates_use_significant_context_and_keyword_classification() {
    let mut d = definition();
    d["regions"]["implicit"]["token_rules"] = serde_json::json!(["word", "punctuation"]);
    d["regions"]["implicit"]["close_before"][0]["after_kinds"] = serde_json::json!(["word"]);
    let tokens = lexer(d.clone()).tokenize("@;body;tail").unwrap();
    assert_eq!(tokens[0].text, "@;body");
    d["keywords"] = serde_json::json!(["go"]);
    d["regions"]["implicit"]["close_before"][0]["after_kinds"] = serde_json::json!(["keyword"]);
    assert_eq!(lexer(d).tokenize("@go;tail").unwrap()[0].text, "@go");
}
#[test]
fn escapes_precede_boundaries_and_headers_do_not_consult_body_boundaries() {
    let mut d = definition();
    d["regions"]["implicit"]["literal_escapes"] = serde_json::json!([";;"]);
    d["regions"]["implicit"]["escape"] = "\\".into();
    assert_eq!(
        lexer(d).tokenize("@a;;b\\;c;tail").unwrap()[0].text,
        "@a;;b\\;c"
    );
    let mut d = definition();
    d["regions"]["implicit"]["header"] = serde_json::json!({"end":":"});
    let tokens = lexer(d).tokenize("@a;b:rest;tail").unwrap();
    assert_eq!(tokens[0].text, "@a;b:rest");
    assert!(tokens[0].regions[0].header_closed);
}
#[test]
fn definition_limits_bound_pattern_width_rule_counts_kinds_and_actual_work() {
    let mut d = definition();
    d["regions"]["implicit"]["close_before"][0]["pattern"] = ".{32}".into();
    assert!(Lexer::new(&serde_json::from_value(d.clone()).unwrap()).is_ok());
    d["regions"]["implicit"]["close_before"][0]["pattern"] = ".{33}".into();
    assert!(Lexer::new(&serde_json::from_value(d).unwrap()).is_err());
    for kinds in [
        serde_json::json!([""]),
        serde_json::json!(["missing"]),
        serde_json::json!(["word", "word"]),
    ] {
        let mut d = definition();
        d["regions"]["implicit"]["close_before"][0]["after_kinds"] = kinds;
        assert!(Lexer::new(&serde_json::from_value(d).unwrap()).is_err());
    }
    let mut d = definition();
    d["regions"]["implicit"]["close_before"] = serde_json::json!(
        (0..17)
            .map(|_| serde_json::json!({"pattern":"a"}))
            .collect::<Vec<_>>()
    );
    assert!(Lexer::new(&serde_json::from_value(d).unwrap()).is_err());
    let mut d = definition();
    d["max_region_steps"] = 3.into();
    assert!(lexer(d).tokenize("@longbody;tail").is_err());
    let mut d = definition();
    for index in 0..8 {
        d["regions"][format!("extra{index}")] = serde_json::json!({"close":"!","close_before":(0..16).map(|_|serde_json::json!({"pattern":"a"})).collect::<Vec<_>>()});
    }
    d["regions"]["implicit"]["close_before"] = serde_json::json!([]);
    d["regions"]["implicit"]["eof_closes"] = true.into();
    assert!(Lexer::new(&serde_json::from_value(d.clone()).unwrap()).is_ok());
    d["regions"]["extra"] = serde_json::json!({"close":"!","close_before":[{"pattern":"a"}]});
    assert!(Lexer::new(&serde_json::from_value(d).unwrap()).is_err());
}
