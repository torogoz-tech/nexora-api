use nexora_lexer::Lexer;
fn definition() -> serde_json::Value {
    serde_json::json!({"schema_version":13,"identifier_kind":"word","keywords":["if"],"entry_contexts":{"dialect":["word","note","gap"],"strict":["word","gap"]},"tokens":[{"kind":"note","pattern":"@\\*","region":"note","root":false,"trivia":true},{"kind":"word","pattern":"[\\p{L}]+"},{"kind":"gap","pattern":"\\s+","trivia":true}],"regions":{"note":{"close":"*@"}}})
}
#[test]
fn explicit_entry_uses_local_rules_and_keeps_the_default_root_strict() {
    let lexer = Lexer::new(&serde_json::from_value(definition()).unwrap()).unwrap();
    let source = "if @* hidden *@ value";
    assert!(lexer.tokenize(source).is_err());
    assert!(lexer.tokenize_in_context(source, "strict").is_err());
    let tokens = lexer.tokenize_in_context(source, "dialect").unwrap();
    assert_eq!(
        tokens
            .iter()
            .filter(|t| !t.trivia)
            .map(|t| t.kind.as_str())
            .collect::<Vec<_>>(),
        ["keyword", "word"]
    );
    let note = tokens.iter().find(|t| t.kind == "note").unwrap();
    assert!(note.trivia && note.regions[0].closed);
    assert!(lexer.tokenize_in_context("value", "missing").is_err());
}
#[test]
fn entry_list_does_not_override_definition_order_and_ranges_remain_absolute() {
    let mut d = definition();
    d["tokens"].as_array_mut().unwrap().insert(
        0,
        serde_json::json!({"kind":"special","pattern":"[\\p{L}]+","root":false}),
    );
    d["entry_contexts"]["priority"] = serde_json::json!(["word", "special"]);
    let lexer = Lexer::new(&serde_json::from_value(d).unwrap()).unwrap();
    assert_eq!(
        lexer.tokenize_in_context("écho", "priority").unwrap()[0].kind,
        "special"
    );
    assert_eq!(lexer.tokenize("écho").unwrap()[0].kind, "word");
    let source = "outside écho beyond";
    let start = source.find('é').unwrap();
    let end = start + "écho".len();
    let index = nexora_core::SourcePositionIndex::new(source).unwrap();
    let tokens = lexer
        .tokenize_indexed_range_in_context(&index, start..end, Some("priority"))
        .unwrap();
    assert_eq!(tokens[0].text, "écho");
    assert_eq!(tokens[0].span.start.offset, start);
    assert_eq!(tokens[0].span.end.offset, end);
    assert!(
        lexer
            .tokenize_indexed_range_in_context(&index, start + 1..end, Some("priority"))
            .is_err()
    );
}
#[test]
fn entries_require_schema_thirteen_and_valid_distinct_declared_rules() {
    let mut d = definition();
    d["schema_version"] = 12.into();
    assert!(Lexer::new(&serde_json::from_value(d.clone()).unwrap()).is_err());
    d.as_object_mut().unwrap().remove("entry_contexts");
    assert!(Lexer::new(&serde_json::from_value(d).unwrap()).is_ok());
    for list in [
        serde_json::json!([]),
        serde_json::json!(["missing"]),
        serde_json::json!(["word", "word"]),
    ] {
        let mut d = definition();
        d["entry_contexts"]["dialect"] = list;
        assert!(Lexer::new(&serde_json::from_value(d).unwrap()).is_err());
    }
    let mut d = definition();
    d["entry_contexts"][""] = serde_json::json!(["word"]);
    assert!(Lexer::new(&serde_json::from_value(d).unwrap()).is_err());
}
#[test]
fn expanded_entry_reference_and_entry_count_limits_are_enforced() {
    let mut d = definition();
    d["tokens"] = serde_json::json!(
        (0..128)
            .map(|i| serde_json::json!({"kind":format!("kind{i}"),"pattern":format!("word{i}")}))
            .collect::<Vec<_>>()
    );
    let all = serde_json::json!((0..128).map(|i| format!("kind{i}")).collect::<Vec<_>>());
    let mut entries = serde_json::Map::new();
    for i in 0..8 {
        entries.insert(format!("entry{i}"), all.clone());
    }
    d["entry_contexts"] = serde_json::Value::Object(entries);
    assert!(Lexer::new(&serde_json::from_value(d.clone()).unwrap()).is_ok());
    d["entry_contexts"]["overflow"] = serde_json::json!(["kind0"]);
    assert!(Lexer::new(&serde_json::from_value(d).unwrap()).is_err());
    let mut d = definition();
    let mut entries = serde_json::Map::new();
    for i in 0..65 {
        entries.insert(format!("entry{i}"), serde_json::json!(["word"]));
    }
    d["entry_contexts"] = serde_json::Value::Object(entries);
    assert!(Lexer::new(&serde_json::from_value(d).unwrap()).is_err());
}
#[test]
fn duplicate_token_kinds_still_charge_every_compiled_entry_reference() {
    let mut d = definition();
    d["tokens"] = serde_json::json!(
        (0..128)
            .map(|i| serde_json::json!({"kind":"word","pattern":format!("word{i}")}))
            .collect::<Vec<_>>()
    );
    let mut entries = serde_json::Map::new();
    for i in 0..8 {
        entries.insert(format!("entry{i}"), serde_json::json!(["word"]));
    }
    d["entry_contexts"] = serde_json::Value::Object(entries);
    assert!(Lexer::new(&serde_json::from_value(d.clone()).unwrap()).is_ok());
    d["entry_contexts"]["overflow"] = serde_json::json!(["word"]);
    assert!(Lexer::new(&serde_json::from_value(d).unwrap()).is_err());
    let mut d = definition();
    let mut entries = serde_json::Map::new();
    for i in 0..64 {
        entries.insert(format!("entry{i}"), serde_json::json!(["word"]));
    }
    d["entry_contexts"] = serde_json::Value::Object(entries);
    assert!(Lexer::new(&serde_json::from_value(d.clone()).unwrap()).is_ok());
    d["entry_contexts"]["overflow"] = serde_json::json!(["word"]);
    assert!(Lexer::new(&serde_json::from_value(d).unwrap()).is_err());
}
