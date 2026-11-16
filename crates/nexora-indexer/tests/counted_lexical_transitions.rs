use nexora_core::NexoraError;
use nexora_language::LexerDefinition;
use nexora_lexer::Lexer;
use serde_json::{Value, json};

fn model() -> Value {
    json!({"schema_version":14,"identifier_kind":"word","keywords":[],
    "tokens":[{"kind":"raw","pattern":"(?P<arity>\\$+)~","region":"raw"},{"kind":"word","pattern":"[a-z]+"},{"kind":"space","pattern":"\\s+","trivia":true}],
    "regions":{
        "raw":{"close":"~","counted_children":[{"capture":"arity","unit":"$","open":"{","region":"hole","open_at_end":true,"max_run_factor":2,"reject_unmatched_close":true}]},
        "hole":{"close":"}","header":{"end":":","terminal":"}","children":[{"pattern":"\\\"","region":"quote"},{"pattern":"\\{","region":"group"},{"pattern":"(?P<arity>\\$+)~","region":"raw"}]}},
        "quote":{"close":"\"","escape":"\\"},
        "group":{"close":"}","children":[{"pattern":"\\{","region":"group"},{"pattern":"\\\"","region":"quote"}]}
    }})
}
fn lexer(value: Value) -> Lexer {
    let definition: LexerDefinition = serde_json::from_value(value).unwrap();
    Lexer::new(&definition).unwrap()
}

#[test]
fn parent_capture_counts_delimiters_and_preserves_literal_prefixes_and_neighbors() {
    for n in [1, 2, 3, 16] {
        let prefix = "{".repeat(n - 1);
        let source = format!(
            "{}~{prefix} text {}v{}{prefix}~ tail",
            "$".repeat(n),
            "{".repeat(n),
            "}".repeat(n)
        );
        let tokens = lexer(model()).tokenize(&source).unwrap();
        assert_eq!(tokens.last().unwrap().text, "tail");
        let regions = &tokens[0].regions;
        assert!(regions.iter().all(|r| r.closed));
        assert_eq!(regions.len(), 2);
        let hole = &regions[1];
        assert_eq!(
            &source[hole.opener.start.offset..hole.header_content_span.unwrap().start.offset],
            "{".repeat(n)
        );
        let closer = hole.closer.unwrap();
        assert_eq!(
            &source[closer.start.offset..closer.end.offset],
            "}".repeat(n)
        );
        assert_eq!(hole.parent.as_ref(), Some(&regions[0].id));
    }
    let source = "$$~{{{v}}}~ tail";
    let tokens = lexer(model()).tokenize(source).unwrap();
    assert_eq!(
        tokens[0].regions[1].opener.start.offset,
        source.find("{{{").unwrap() + 1
    );
}

#[test]
fn different_multibyte_units_use_exact_repetitions_and_original_positions() {
    let mut value = model();
    value["tokens"][0]["pattern"] = json!("(?P<arity>(?:éé)+)~");
    value["regions"]["raw"]["counted_children"][0]["unit"] = json!("éé");
    value["regions"]["raw"]["counted_children"][0]["open"] = json!("«");
    value["regions"]["hole"]["close"] = json!("»");
    value["regions"]["hole"]["header"]["terminal"] = json!("»");
    value["regions"]["hole"]["header"]["children"] = json!([]);
    let source = "éééé~« text ««value»»~ tail";
    let tokens = lexer(value).tokenize(source).unwrap();
    let hole = &tokens[0].regions[1];
    assert_eq!(hole.opener.start.offset, source.find("««").unwrap());
    assert_eq!(
        hole.opener.start.column,
        source[..hole.opener.start.offset].chars().count() + 1
    );
    assert_eq!(
        &source[hole.opener.start.offset..hole.header_content_span.unwrap().start.offset],
        "««"
    );
    assert_eq!(tokens.last().unwrap().text, "tail");
}

#[test]
fn header_terminal_format_and_nested_guards_use_their_own_counts() {
    for source in [
        "$$~{{f(\"}}\", {x}):fmt}x}}~ tail",
        "$$~{{f($~{v}~)}}~ tail",
    ] {
        let tokens = lexer(model()).tokenize(source).unwrap();
        assert!(
            tokens[0]
                .regions
                .iter()
                .all(|r| r.closed && (r.header.is_none() || r.header_closed))
        );
        assert_eq!(tokens.last().unwrap().text, "tail");
    }
    let source = "$$~{{value:fmt}x}}~ tail";
    let tokens = lexer(model()).tokenize(source).unwrap();
    let hole = &tokens[0].regions[1];
    assert_eq!(
        &source[hole.content_span.start.offset..hole.content_span.end.offset],
        "fmt}x"
    );
    let tokens = lexer(model()).tokenize("$$~{{unfinished").unwrap();
    assert!(tokens[0].regions.iter().all(|r| !r.closed));
}

#[test]
fn malformed_counts_and_long_runs_are_explicit_and_bounded() {
    let l = lexer(model());
    for source in ["$$~{{{{value}}~", "$$~}}~", "$~{{v}~"] {
        assert!(matches!(l.tokenize(source), Err(NexoraError::Lexical(_))));
    }
    assert!(matches!(
        l.tokenize(&format!("{}~", "$".repeat(257))),
        Err(NexoraError::Limit(_))
    ));
    for field in ["max_region_steps", "max_region_nodes", "max_region_depth"] {
        let mut value = model();
        value[field] = json!(1);
        assert!(matches!(
            lexer(value).tokenize("$~{v}~"),
            Err(NexoraError::Limit(_))
        ));
    }
    let mut value = model();
    value["tokens"][0]["pattern"] = json!("(?P<arity>\\$x)~");
    assert!(matches!(
        lexer(value).tokenize("$x~"),
        Err(NexoraError::Configuration(_))
    ));
}

#[test]
fn contracts_reject_unknown_captures_versions_and_alternate_child_closures() {
    for (field, invalid) in [
        ("capture", json!("absent")),
        ("unit", json!("")),
        ("open", json!("")),
        ("region", json!("missing")),
        ("max_run_factor", json!(1)),
        ("max_run_factor", json!(17)),
    ] {
        let mut value = model();
        value["regions"]["raw"]["counted_children"][0][field] = invalid;
        let definition: LexerDefinition = serde_json::from_value(value).unwrap();
        assert!(Lexer::new(&definition).is_err(), "{field}");
    }
    for (field, invalid) in [
        ("alternate_closes", json!(["!"])),
        ("close_before", json!([{"pattern":"!"}])),
        ("eof_closes", json!(true)),
        ("close_capture", json!("arity")),
        ("close_tail", json!("x")),
    ] {
        let mut value = model();
        value["regions"]["hole"][field] = invalid;
        let definition: LexerDefinition = serde_json::from_value(value).unwrap();
        assert!(Lexer::new(&definition).is_err(), "{field}");
    }
    let mut value = model();
    value["schema_version"] = json!(13);
    let definition: LexerDefinition = serde_json::from_value(value).unwrap();
    assert!(Lexer::new(&definition).is_err());
}

#[test]
fn optional_and_empty_captures_do_not_silently_supply_a_count() {
    for pattern in ["(?P<arity>\\$+)?!", "(?P<arity>\\$*)!"] {
        let mut value = model();
        value["tokens"][0]["pattern"] = json!(pattern);
        assert!(matches!(
            lexer(value).tokenize("!"),
            Err(NexoraError::Configuration(_))
        ));
    }
    let mut value = model();
    value["tokens"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"local","pattern":"!","region":"raw","root":false}));
    let definition: LexerDefinition = serde_json::from_value(value).unwrap();
    assert!(Lexer::new(&definition).is_err());
}

#[test]
fn rejected_opening_follow_does_not_consume_a_literal_prefix() {
    let mut value = model();
    value["regions"]["hole"]["opening_follow"] = json!("[a-z]");
    value["regions"]["raw"]["counted_children"][0]["reject_unmatched_close"] = json!(false);
    value["regions"]["raw"]["children"] = json!([{"pattern":"\\{","region":"group"}]);
    let source = "$$~{{{1}}}~ tail";
    let tokens = lexer(value).tokenize(source).unwrap();
    assert!(tokens[0].regions.iter().all(|r| r.mode != "hole"));
    assert_eq!(
        tokens[0].regions[1].opener.start.offset,
        source.find('{').unwrap()
    );
    assert!(tokens[0].regions.iter().all(|r| r.closed));
}

#[test]
fn bounded_input_and_materialization_budgets_do_not_borrow_external_bytes() {
    let source = "é\n$$~{{value}}~ tail";
    let end = source.find("}}").unwrap() + 1;
    let tokens = lexer(model()).tokenize_range(source, 3..end).unwrap();
    assert!(
        tokens[0]
            .regions
            .iter()
            .all(|r| !r.closed && r.span.end.offset == end)
    );
    assert_eq!(tokens[0].span.start.line, 2);
    let tokens = lexer(model())
        .tokenize(&format!(
            "{}~{}v{}~ tail",
            "$".repeat(256),
            "{".repeat(256),
            "}".repeat(256)
        ))
        .unwrap();
    assert!(tokens[0].regions.iter().all(|r| r.closed));
    let mut value = model();
    value["regions"]["raw"]["counted_children"][0]["open"] = json!("éé");
    assert!(matches!(
        lexer(value).tokenize(&format!("{}~", "$".repeat(65))),
        Err(NexoraError::Limit(_))
    ));
    let mut value = model();
    value["regions"]["raw"]["counted_children"][0]["max_run_factor"] = Value::Null;
    value["max_region_steps"] = json!(32);
    assert!(matches!(
        lexer(value).tokenize(&format!("$$~{}", "{".repeat(100_000))),
        Err(NexoraError::Limit(_))
    ));
}

#[test]
fn counted_and_regex_transitions_share_the_expanded_reference_quota() {
    let mut value = model();
    value["regions"].as_object_mut().unwrap().remove("quote");
    value["regions"].as_object_mut().unwrap().remove("group");
    value["regions"]["hole"]["header"]["children"] = json!([]);
    let transition = value["regions"]["raw"]["counted_children"][0].clone();
    value["regions"]["raw"]["counted_children"] = json!(
        (0..16)
            .map(|i| {
                let mut t = transition.clone();
                t["open"] = json!(format!("o{i}"));
                t
            })
            .collect::<Vec<_>>()
    );
    let parent = value["regions"]["raw"].clone();
    for i in 0..62 {
        value["regions"][format!("parent{i}")] = parent.clone();
    }
    value["regions"]["raw"]["children"] = json!(
        (0..16)
            .map(|i| json!({"pattern":format!("X{i}"),"region":"hole"}))
            .collect::<Vec<_>>()
    );
    let definition: LexerDefinition = serde_json::from_value(value.clone()).unwrap();
    assert!(Lexer::new(&definition).is_ok()); // 63*16 + 16 = 1024
    value["regions"]["raw"]["children"]
        .as_array_mut()
        .unwrap()
        .push(json!({"pattern":"overflow","region":"hole"}));
    let definition: LexerDefinition = serde_json::from_value(value).unwrap();
    assert!(Lexer::new(&definition).is_err());
}

#[test]
fn empty_counted_tables_preserve_old_schema_and_serialization() {
    let mut value = model();
    value["schema_version"] = json!(13);
    value["regions"]["raw"]["counted_children"] = json!([]);
    let definition: LexerDefinition = serde_json::from_value(value).unwrap();
    let encoded = serde_json::to_value(&definition).unwrap();
    assert!(
        encoded["regions"]
            .as_object()
            .unwrap()
            .values()
            .all(|r| r.get("counted_children").is_none())
    );
    let tokens = Lexer::new(&definition)
        .unwrap()
        .tokenize("$$~{{value}}~ tail")
        .unwrap();
    assert_eq!(tokens[0].regions.len(), 1);
    assert_eq!(tokens.last().unwrap().text, "tail");
}
