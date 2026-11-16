use nexora_language::LexerDefinition;
use nexora_lexer::Lexer;
fn definition() -> serde_json::Value {
    serde_json::json!({"schema_version":10,"identifier_kind":"word","keywords":[],"tokens":[{"kind":"wrapper","pattern":"\\{","region":"wrapper"},{"kind":"local_long","pattern":"@[a-z]+","root":false},{"kind":"marker","pattern":"@"},{"kind":"word","pattern":"[a-z]+"}],"regions":{"wrapper":{"close":"}","token_rules":["local_long","word"]}}})
}
#[test]
fn mode_only_rules_never_steal_root_tokens_but_remain_available_in_selected_modes() {
    let d: LexerDefinition = serde_json::from_value(definition()).unwrap();
    let lexer = Lexer::new(&d).unwrap();
    let tokens = lexer.tokenize("@value{@local}").unwrap();
    assert_eq!(
        tokens.iter().map(|t| t.kind.as_str()).collect::<Vec<_>>(),
        ["marker", "word", "wrapper"]
    );
    assert!(tokens[2].regions[0].closed);
}
#[test]
fn local_kind_return_still_drives_nested_previous_kind_selection() {
    let mut d = definition();
    d["tokens"].as_array_mut().unwrap().push(serde_json::json!({"kind":"after_local","pattern":"!","root":false,"after_kinds":["local_long"],"at_start":false}));
    d["regions"]["wrapper"]["token_rules"] = serde_json::json!(["local_long", "after_local"]);
    let lexer = Lexer::new(&serde_json::from_value(d).unwrap()).unwrap();
    assert!(lexer.tokenize("{@local!}").unwrap()[0].regions[0].closed);
    assert!(lexer.tokenize("!").is_err());
}
#[test]
fn root_scope_is_versioned_and_older_default_rules_keep_their_behavior() {
    let mut d = definition();
    d["schema_version"] = 9.into();
    assert!(Lexer::new(&serde_json::from_value(d.clone()).unwrap()).is_err());
    d["tokens"][1].as_object_mut().unwrap().remove("root");
    let lexer = Lexer::new(&serde_json::from_value(d).unwrap()).unwrap();
    assert_eq!(lexer.tokenize("@value").unwrap()[0].kind, "local_long");
}
