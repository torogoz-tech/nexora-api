use nexora_language::LexerDefinition;
use nexora_lexer::Lexer;
fn definition(version: u32, count: usize, shared: bool) -> LexerDefinition {
    let mut regions = serde_json::Map::new();
    for mode in 0..count.div_ceil(32) {
        let start = mode * 32;
        let children:Vec<_>=(start..(start+32).min(count)).map(|i|serde_json::json!({"pattern":format!("child{}",if shared {i%32} else {i}),"region":"mode0"})).collect();
        regions.insert(
            format!("mode{mode}"),
            serde_json::json!({"close":">","children":children}),
        );
    }
    serde_json::from_value(serde_json::json!({"schema_version":version,"identifier_kind":"word","keywords":[],"tokens":[{"kind":"word","pattern":"[a-z]+"}],"regions":regions})).unwrap()
}
#[test]
fn shared_phase_references_have_separate_versioned_bounds_from_distinct_definitions() {
    assert!(Lexer::new(&definition(11, 256, true)).is_ok());
    assert!(Lexer::new(&definition(11, 257, true)).is_err());
    assert!(Lexer::new(&definition(12, 1024, true)).is_ok());
    assert!(Lexer::new(&definition(12, 1025, true)).is_err());
    assert!(Lexer::new(&definition(12, 256, false)).is_ok());
    assert!(Lexer::new(&definition(12, 257, false)).is_err());
}
#[test]
fn target_and_rejection_remain_part_of_transition_identity_and_phase_duplicates_stay_invalid() {
    let mut d = definition(12, 256, false);
    d.regions.insert(
        "extra".into(),
        serde_json::from_value(
            serde_json::json!({"close":">","children":[{"pattern":"child0","region":"extra"}]}),
        )
        .unwrap(),
    );
    assert!(Lexer::new(&d).is_err());
    d.regions.get_mut("extra").unwrap().children[0].region = "mode0".into();
    assert!(Lexer::new(&d).is_ok());
    d.regions.get_mut("extra").unwrap().children[0].reject = true;
    assert!(Lexer::new(&d).is_err());
    let mut d = definition(12, 31, true);
    let duplicate = d.regions["mode0"].children[0].clone();
    d.regions.get_mut("mode0").unwrap().children.push(duplicate);
    assert!(Lexer::new(&d).is_err());
}
#[test]
fn shared_regex_keeps_mode_targets_rejections_and_capture_validation_independent() {
    let d:LexerDefinition=serde_json::from_value(serde_json::json!({"schema_version":12,"identifier_kind":"word","keywords":[],"tokens":[{"kind":"left","pattern":"\\{","region":"left"},{"kind":"right","pattern":"\\(","region":"right"},{"kind":"reject","pattern":"\\[","region":"reject"}],"regions":{"left":{"close":"}","children":[{"pattern":"@","region":"first"}]},"right":{"close":")","children":[{"pattern":"@","region":"second"}]},"reject":{"close":"]","children":[{"pattern":"@","region":"first","reject":true}]},"first":{"close":"]"},"second":{"close":">"}}})).unwrap();
    let lexer = Lexer::new(&d).unwrap();
    let tokens = lexer.tokenize("{@x]}(@y>)").unwrap();
    assert!(tokens.iter().flat_map(|t| &t.regions).all(|r| r.closed));
    assert_eq!(tokens[0].regions[1].mode, "first");
    assert_eq!(tokens[1].regions[1].mode, "second");
    assert!(lexer.tokenize("[@x]]").is_err());
    let mut value = serde_json::to_value(d).unwrap();
    value["regions"]["left"]["children"][0]["pattern"] = "(?P<tag>x)".into();
    value["regions"]["right"]["children"][0]["pattern"] = "(?P<tag>x)".into();
    value["regions"]["first"]["close_capture"] = "tag".into();
    value["regions"]["second"]["close_capture"] = "missing".into();
    let invalid: LexerDefinition = serde_json::from_value(value).unwrap();
    assert!(Lexer::new(&invalid).is_err());
}
#[test]
fn excessive_root_rules_are_rejected_before_pattern_preparation() {
    let mut d = definition(12, 1, true);
    let bad: nexora_language::TokenRule =
        serde_json::from_value(serde_json::json!({"kind":"word","pattern":"(","region":"mode0"}))
            .unwrap();
    d.tokens = vec![bad; 129];
    assert!(matches!(
        nexora_language::LexerContextValidator::validate(&d),
        Err(nexora_core::NexoraError::Limit(_))
    ));
}
#[test]
fn shared_modes_keep_lexical_evidence_and_execution_depth_budgets() {
    let mut shared = definition(12, 1024, true);
    shared.tokens.push(
        serde_json::from_value(
            serde_json::json!({"kind":"wrapper","pattern":"\\{","region":"mode0"}),
        )
        .unwrap(),
    );
    let source = "{child0>>";
    let actual = Lexer::new(&shared).unwrap().tokenize(source).unwrap();
    let mut old = definition(11, 256, true);
    old.tokens = shared.tokens.clone();
    let expected = Lexer::new(&old).unwrap().tokenize(source).unwrap();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    shared.max_region_depth = 1;
    assert!(matches!(
        Lexer::new(&shared).unwrap().tokenize(source),
        Err(nexora_core::NexoraError::Limit(_))
    ));
}
#[test]
fn sharing_does_not_relax_mode_or_individual_phase_limits() {
    let mut d = definition(12, 33, false);
    let extra = d.regions["mode1"].children[0].clone();
    d.regions.get_mut("mode0").unwrap().children.push(extra);
    assert!(Lexer::new(&d).is_err());
    let mut d = definition(12, 1, true);
    for i in 1..64 {
        d.regions.insert(
            format!("mode{i}"),
            serde_json::from_value(serde_json::json!({"close":">"})).unwrap(),
        );
    }
    assert!(Lexer::new(&d).is_ok());
    d.regions.insert(
        "extra".into(),
        serde_json::from_value(serde_json::json!({"close":">"})).unwrap(),
    );
    assert!(Lexer::new(&d).is_err());
    let mut d = definition(12, 1, true);
    let children: Vec<_> = (0..33)
        .map(|i| serde_json::json!({"pattern":format!("header{i}"),"region":"mode0"}))
        .collect();
    d.regions.get_mut("mode0").unwrap().header =
        Some(serde_json::from_value(serde_json::json!({"end":"!","children":children})).unwrap());
    assert!(Lexer::new(&d).is_err());
}
