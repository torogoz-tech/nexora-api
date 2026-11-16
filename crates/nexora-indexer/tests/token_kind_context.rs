use nexora_language::LexerDefinition;
use nexora_lexer::Lexer;

fn definition() -> serde_json::Value {
    serde_json::json!({
        "schema_version":8,"identifier_kind":"word","keywords":["go"],
        "tokens":[
            {"kind":"wrapper","pattern":"\\{","region":"wrapper"},
            {"kind":"payload","pattern":"\\[","region":"payload"},
            {"kind":"angle","pattern":"<","region":"angle","after_kinds":["payload"],"after":[{"text":"go","kind":"keyword"}],"at_start":false},
            {"kind":"word","pattern":"[a-z]+"},
            {"kind":"space","pattern":"\\s+","trivia":true},
            {"kind":"operator","pattern":"[<>]"}],
        "regions":{"wrapper":{"close":"}","token_rules":["payload","angle","word","space","operator"]},"payload":{"close":"]"},"angle":{"close":">"}}
    })
}
fn lexer(value: serde_json::Value) -> Lexer {
    let definition: LexerDefinition = serde_json::from_value(value).unwrap();
    Lexer::new(&definition).unwrap()
}
#[test]
fn previous_kind_matches_variable_length_regions_and_unions_with_exact_text_selectors() {
    let lexer = lexer(definition());
    for source in [
        "[first arbitrary body] <one>",
        "[second <different body>]\n<two>",
        "go <three>",
    ] {
        let tokens = lexer.tokenize(source).unwrap();
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == "angle" && t.regions[0].closed)
        );
    }
    let tokens = lexer.tokenize("word <ordinary>").unwrap();
    assert!(!tokens.iter().any(|t| t.kind == "angle"));
    let tokens = lexer.tokenize("<ordinary>").unwrap();
    assert!(!tokens.iter().any(|t| t.kind == "angle"));
}
#[test]
fn nested_region_return_kind_enables_only_the_configured_following_transition() {
    let tokens = lexer(definition())
        .tokenize("{ [arbitrary content] <markup> word <generic> }")
        .unwrap();
    let regions = &tokens[0].regions;
    assert_eq!(regions.iter().filter(|r| r.mode == "angle").count(), 1);
    assert!(regions.iter().all(|r| r.closed));
    assert_eq!(
        regions
            .iter()
            .find(|r| r.mode == "angle")
            .unwrap()
            .span
            .start
            .offset,
        22
    );
}
#[test]
fn keyword_kind_and_start_policy_are_independent_of_text_spelling() {
    let mut d = definition();
    d["tokens"][2]["after"] = serde_json::json!([]);
    d["tokens"][2]["after_kinds"] = serde_json::json!(["keyword"]);
    d["tokens"][2]["at_start"] = true.into();
    let lexer = lexer(d);
    for source in ["<start>", "go <keyword>"] {
        assert!(
            lexer
                .tokenize(source)
                .unwrap()
                .iter()
                .any(|t| t.kind == "angle")
        );
    }
    assert!(
        !lexer
            .tokenize("word <ordinary>")
            .unwrap()
            .iter()
            .any(|t| t.kind == "angle")
    );
}
#[test]
fn context_rejection_and_bounded_ranges_still_apply() {
    let mut d = definition();
    d["tokens"][2]["reject_on_context_mismatch"] = true.into();
    assert!(lexer(d).tokenize("word <ordinary>").is_err());
    let tokens = lexer(definition())
        .tokenize_range("skip [body] <tag> suffix", 5..14)
        .unwrap();
    let angle = tokens.iter().find(|t| t.kind == "angle").unwrap();
    assert_eq!(angle.text, "<t");
    assert!(!angle.regions[0].closed);
    let tokens = lexer(definition())
        .tokenize_range("[body] <tag>", 7..12)
        .unwrap();
    assert!(
        !tokens.iter().any(|t| t.kind == "angle"),
        "previous context outside the range must not leak in"
    );
}
#[test]
fn kind_selectors_are_versioned_declared_distinct_and_bounded() {
    for kinds in [
        serde_json::json!([""]),
        serde_json::json!(["unknown"]),
        serde_json::json!(["space"]),
        serde_json::json!(["word", "word"]),
        serde_json::json!((0..33).map(|_| "word").collect::<Vec<_>>()),
    ] {
        let mut d = definition();
        d["tokens"][2]["after_kinds"] = kinds;
        let definition: LexerDefinition = serde_json::from_value(d).unwrap();
        assert!(Lexer::new(&definition).is_err());
    }
    let mut d = definition();
    d["schema_version"] = 7.into();
    let definition: LexerDefinition = serde_json::from_value(d.clone()).unwrap();
    assert!(Lexer::new(&definition).is_err());
    d["tokens"][2]
        .as_object_mut()
        .unwrap()
        .remove("after_kinds");
    assert!(Lexer::new(&serde_json::from_value(d).unwrap()).is_ok());
}

#[test]
fn kind_selector_byte_limit_is_enforced_even_when_the_kind_is_declared() {
    for kind in ["".to_owned(), "k".repeat(129), "é".repeat(65)] {
        let mut d = definition();
        d["tokens"][3]["kind"] = kind.clone().into();
        d["identifier_kind"] = kind.clone().into();
        d["regions"]["wrapper"]["token_rules"] =
            serde_json::json!(["payload", "angle", kind, "space", "operator"]);
        d["tokens"][2]["after_kinds"] = serde_json::json!([kind]);
        let definition: LexerDefinition = serde_json::from_value(d).unwrap();
        assert!(Lexer::new(&definition).is_err());
    }
}

#[test]
fn direct_child_return_kind_uses_the_same_selector_contract() {
    let mut d = definition();
    d["regions"]["wrapper"]["children"] = serde_json::json!([{"pattern":"\\[","region":"payload"}]);
    d["regions"]["payload"]["token_kind"] = "payload".into();
    let tokens = lexer(d)
        .tokenize("{ [returned child of arbitrary length] <tag> }")
        .unwrap();
    assert_eq!(
        tokens[0]
            .regions
            .iter()
            .filter(|r| r.mode == "angle")
            .count(),
        1
    );
    assert!(tokens[0].regions.iter().all(|r| r.closed));
}

#[test]
fn kind_selector_count_accepts_thirty_two_and_rejects_thirty_three_distinct_declared_kinds() {
    let mut d = definition();
    let kinds: Vec<_> = (0..33).map(|index| format!("kind{index}")).collect();
    for kind in &kinds {
        d["tokens"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"kind":kind,"pattern":"[0-9]+"}));
    }
    d["tokens"][2]["after_kinds"] = serde_json::json!(&kinds[..32]);
    let accepted: LexerDefinition = serde_json::from_value(d.clone()).unwrap();
    assert!(Lexer::new(&accepted).is_ok());
    d["tokens"][2]["after_kinds"] = serde_json::json!(kinds);
    let rejected: LexerDefinition = serde_json::from_value(d).unwrap();
    assert!(Lexer::new(&rejected).is_err());
}
