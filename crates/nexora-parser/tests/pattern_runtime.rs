use nexora_language::{GrammarPattern as P, LanguageDefinition};
use nexora_lexer::Lexer;
use nexora_parser::{Parser, PatternMatcher};
use std::{collections::BTreeMap, path::Path};
fn language() -> LanguageDefinition {
    LanguageDefinition::load(Path::new("../../templates/language-pack")).unwrap()
}
fn capture(item: P) -> P {
    P::Capture {
        name: "name".into(),
        item: Box::new(item),
        join: String::new(),
    }
}
#[test]
fn alternative_failure_rolls_back_captures() {
    let language = language();
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize("alpha")
        .unwrap();
    let significant: Vec<_> = tokens.iter().enumerate().collect();
    let mut matcher = PatternMatcher::new(&language.grammar, &significant);
    let mut captures = BTreeMap::new();
    let pattern = P::Choice {
        alternatives: vec![
            P::Sequence {
                items: vec![
                    capture(P::Token {
                        kind: "identifier".into(),
                    }),
                    P::Literal {
                        text: ";".into(),
                        kind: None,
                    },
                ],
            },
            capture(P::Token {
                kind: "identifier".into(),
            }),
        ],
    };
    assert_eq!(matcher.pattern(&pattern, 0, &mut captures, 0).unwrap(), 1);
    assert_eq!(captures.len(), 1);
    assert_eq!(captures["name"].end, 1);
}
#[test]
fn zero_progress_cycles_unknown_rules_and_missing_captures_are_rejected() {
    let mut language = language();
    language.grammar.ignored = vec![P::LineEnd];
    assert!(language.validate().is_err());
    let mut language = super_language();
    language.grammar.rules.insert(
        "cycle".into(),
        P::Rule {
            name: "cycle".into(),
        },
    );
    assert!(language.validate().is_err());
    let mut language = super_language();
    language.grammar.productions[0].projection.name_capture = "missing".into();
    assert!(language.validate().is_err());
    let mut language = super_language();
    language.grammar.rules.insert(
        "bad".into(),
        P::Repeat {
            item: Box::new(P::LineEnd),
            min: 0,
            max: 4,
        },
    );
    assert!(language.validate().is_err());
}
fn super_language() -> LanguageDefinition {
    language()
}
#[test]
fn template_syntax_and_unicode_spans_need_no_dialect_parser() {
    let language = language();
    let source = "USE infrastructure.storage; DEF α [x] BEGIN BEGIN x FINISH FINISH";
    let tree = Parser::new(language.grammar).parse(
        &Lexer::new(&language.lexer)
            .unwrap()
            .tokenize(source)
            .unwrap(),
    );
    assert!(tree.diagnostics.is_empty(), "{:?}", tree.diagnostics);
    assert_eq!(tree.nodes[0].name, "infrastructure.storage");
    assert_eq!(tree.nodes[1].name, "α");
    let span = tree.nodes[1].span;
    assert_eq!(
        &source[span.start.offset..span.end.offset],
        "DEF α [x] BEGIN BEGIN x FINISH FINISH"
    );
}
#[test]
fn runtime_budgets_prevent_unvalidated_patterns_from_hanging() {
    let language = language();
    let tokens = Lexer::new(&language.lexer).unwrap().tokenize("x").unwrap();
    let significant: Vec<_> = tokens.iter().enumerate().collect();
    let mut matcher = PatternMatcher::new(&language.grammar, &significant);
    assert!(
        matcher
            .pattern(
                &P::Repeat {
                    item: Box::new(P::LineEnd),
                    min: 0,
                    max: 4
                },
                1,
                &mut BTreeMap::new(),
                0
            )
            .unwrap_err()
            .fatal
    );
    let mut grammar = language.grammar;
    grammar.max_steps = 1;
    let tree = Parser::new(grammar).parse(&tokens);
    assert!(!tree.diagnostics.is_empty());
}

#[test]
fn productive_recursive_rules_are_data_and_left_recursion_is_rejected() {
    let mut language = language();
    let expression = P::Choice {
        alternatives: vec![
            P::Token {
                kind: "identifier".into(),
            },
            P::Sequence {
                items: vec![
                    P::Literal {
                        text: "[".into(),
                        kind: None,
                    },
                    P::Rule {
                        name: "expression".into(),
                    },
                    P::Literal {
                        text: "]".into(),
                        kind: None,
                    },
                ],
            },
        ],
    };
    language
        .grammar
        .rules
        .insert("expression".into(), expression);
    language.grammar.productions[0].pattern = P::Sequence {
        items: vec![
            P::Literal {
                text: "DEF".into(),
                kind: None,
            },
            capture(P::Token {
                kind: "identifier".into(),
            }),
            P::Literal {
                text: "[".into(),
                kind: None,
            },
            P::Rule {
                name: "expression".into(),
            },
            P::Literal {
                text: "]".into(),
                kind: None,
            },
            P::Balanced {
                open: "BEGIN".into(),
            },
        ],
    };
    language.validate().unwrap();
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize("DEF sample [[[x]]] BEGIN x FINISH")
        .unwrap();
    let tree = Parser::new(language.grammar.clone()).parse(&tokens);
    assert!(tree.diagnostics.is_empty(), "{:?}", tree.diagnostics);
    assert_eq!(tree.nodes[0].name, "sample");
    language.grammar.rules.insert(
        "left".into(),
        P::Choice {
            alternatives: vec![
                P::Rule {
                    name: "left".into(),
                },
                P::Token {
                    kind: "identifier".into(),
                },
            ],
        },
    );
    assert!(language.validate().is_err());
}

#[test]
fn fatal_ignored_patterns_cannot_be_treated_as_nonmatches() {
    let mut language = language();
    language.grammar.max_nesting = 3;
    let mut ignored = P::Literal {
        text: "X".into(),
        kind: None,
    };
    for _ in 0..6 {
        ignored = P::Sequence {
            items: vec![ignored],
        };
    }
    language.grammar.ignored = vec![ignored];
    language.validate().unwrap();
    let tokens = Lexer::new(&language.lexer)
        .unwrap()
        .tokenize("DEF alpha [] BEGIN FINISH")
        .unwrap();
    let tree = Parser::new(language.grammar).parse(&tokens);
    assert!(!tree.diagnostics.is_empty());
    assert!(tree.nodes.is_empty());
}
