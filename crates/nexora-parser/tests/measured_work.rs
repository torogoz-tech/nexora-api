use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use nexora_parser::Parser;
use std::path::Path;

#[test]
fn measured_context_enforces_external_work_limit_and_preserves_local_ceiling() {
    let language =
        LanguageDefinition::load(Path::new("../../templates/web-understanding/typescript"))
            .unwrap();
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize("value + 1")
        .unwrap();
    let parser = Parser::new(language.grammar.clone());
    let full = parser
        .parse_in_context_measured(&tokens, "expression", Default::default(), usize::MAX)
        .unwrap();
    assert!(full.tree.diagnostics.is_empty());
    assert!(full.steps > 2 && full.steps < 100);
    assert!(!full.budget_exhausted);
    let exact = parser
        .parse_in_context_measured(&tokens, "expression", Default::default(), full.steps)
        .unwrap();
    assert_eq!(exact.steps, full.steps);
    assert!(!exact.budget_exhausted);
    let short = parser
        .parse_in_context_measured(&tokens, "expression", Default::default(), full.steps - 1)
        .unwrap();
    assert_eq!(short.steps, full.steps - 1);
    assert!(short.budget_exhausted);
    assert!(!short.tree.diagnostics.is_empty());
    let zero = parser
        .parse_in_context_measured(&tokens, "expression", Default::default(), 0)
        .unwrap();
    assert_eq!(zero.steps, 0);
    assert!(zero.budget_exhausted);
    let mut local = language.grammar;
    local.max_steps = 2;
    let limited = Parser::new(local)
        .parse_in_context_measured(&tokens, "expression", Default::default(), 5_000)
        .unwrap();
    assert_eq!(limited.steps, 2);
    assert!(limited.budget_exhausted);
    assert!(
        parser
            .parse_in_context_measured(&tokens, "missing", Default::default(), 10)
            .is_err()
    );
}

#[test]
fn measured_root_counts_backtracking_and_recovery_against_the_same_allowance() {
    let language =
        LanguageDefinition::load(Path::new("../../templates/web-understanding/typescript"))
            .unwrap();
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize("unknown ???; const after = 1;")
        .unwrap();
    let parser = Parser::new(language.grammar);
    let full = parser.parse_measured(&tokens, usize::MAX);
    assert!(full.tree.nodes.iter().any(|n| n.name == "after"));
    assert!(!full.budget_exhausted);
    let limited = parser.parse_measured(&tokens, 5);
    assert!(limited.budget_exhausted);
    assert_eq!(limited.steps, 5);
    assert!(!limited.tree.nodes.iter().any(|n| n.name == "after"));
}
