use nexora_language::LexerDefinition;
use nexora_lexer::Lexer;
fn definition(version: u32, count: usize) -> LexerDefinition {
    let mut regions = serde_json::Map::new();
    let mut remaining = count;
    for index in 0..count.div_ceil(32) {
        let children: Vec<_> = (0..remaining.min(32))
            .map(|child| serde_json::json!({"pattern":format!("child{child}"),"region":"mode0"}))
            .collect();
        remaining -= children.len();
        regions.insert(
            format!("mode{index}"),
            serde_json::json!({"close":">","children":children}),
        );
    }
    serde_json::from_value(serde_json::json!({"schema_version":version,"identifier_kind":"word","keywords":[],"tokens":[{"kind":"word","pattern":"[a-z]+"}],"regions":regions})).unwrap()
}
#[test]
fn complex_context_graphs_keep_a_fixed_bound_and_preserve_older_schema_limits() {
    assert!(Lexer::new(&definition(7, 128)).is_ok());
    assert!(Lexer::new(&definition(7, 129)).is_err());
    assert!(Lexer::new(&definition(8, 129)).is_ok());
    assert!(Lexer::new(&definition(8, 256)).is_ok());
    assert!(Lexer::new(&definition(8, 257)).is_err());
}
