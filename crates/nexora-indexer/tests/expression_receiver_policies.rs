use nexora_core::ExpressionKind as K;
use nexora_indexer::{AnalysisResult, Indexer};
use nexora_language::LanguageDefinition;
use serde_json::{Value, json};
use std::{fs, path::Path};

fn language(id: &str) -> LanguageDefinition {
    let mut l =
        LanguageDefinition::load(&Path::new("../../templates/semantic-molds").join(id)).unwrap();
    let mut grammar = serde_json::to_value(&l.grammar).unwrap();
    grammar["schema_version"] = json!(10);
    grammar["contexts"]["expression"]["expression"]["application"][0]["excluded_receivers"] = json!([{"kinds":["group"],"descendant_kinds":["identifier","member"],"descendant_operators":["."]}]);
    l.grammar = serde_json::from_value(grammar).unwrap();
    let mut graph = serde_json::to_value(&l.graph).unwrap();
    graph["schema_version"] = json!(5);
    graph["expression_bindings"] = json!({"expression":{"target_kinds":["Function","Variable","Parameter"],"callable_kinds":["Function"],"unwrap_groups":true,"mutating_unary_operators":["++"]}});
    l.graph = serde_json::from_value(graph).unwrap();
    l.validate().unwrap();
    l
}
fn analyze(l: LanguageDefinition, source: &str, name: &str) -> AnalysisResult {
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-receiver-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("public.{}", l.manifest.extensions[0]));
    fs::write(&file, source).unwrap();
    let result = Indexer::new(l).analyze(&file).unwrap();
    fs::remove_dir_all(dir).unwrap();
    result
}

#[test]
fn two_syntaxes_reject_only_the_declared_receiver_without_losing_neighbors() {
    for (id, source) in [
        (
            "brace_mold",
            "FUNC f(p){EVAL p;} VAR value=(f)(1); VAR after=2;",
        ),
        (
            "keyword_mold",
            "DEF f[p] BEGIN VALUE p; FINISH LET value BECOMES [f][1]; LET after BECOMES 2;",
        ),
    ] {
        let result = analyze(language(id), source, id);
        assert!(result.diagnostics.iter().any(|d| d.stage == "parser"));
        assert!(
            result
                .graph
                .nodes
                .iter()
                .any(|n| n.name == "after" && n.kind == nexora_graph::NodeKind::Variable)
        );
        assert!(
            result
                .graph
                .edges
                .iter()
                .all(|e| e.kind != nexora_graph::EdgeKind::Calls)
        );
        assert!(result.ast.nodes.iter().all(|n| {
            n.expression
                .as_ref()
                .is_none_or(|s| s.kind != K::Application)
        }));
    }
}

#[test]
fn descendant_sets_distinguish_qualified_names_from_nested_or_computed_groups() {
    for (text, rejected) in [
        ("(a.b.c)(1)", true),
        ("(a)(1)", true),
        ("((a))(1)", false),
        ("(a+b)(1)", false),
        ("f(1)", false),
    ] {
        let source = format!("VAR a; VAR b; VAR c; VAR f; VAR value={text};");
        let result = analyze(
            language("brace_mold"),
            &source,
            &format!("shape-{}", text.len()),
        );
        assert_eq!(
            result.diagnostics.iter().any(|d| d.stage == "parser"),
            rejected,
            "{text}"
        );
        assert_eq!(
            result.ast.nodes.iter().any(|n| n
                .expression
                .as_ref()
                .is_some_and(|s| s.kind == K::Application)),
            !rejected,
            "{text}"
        );
    }
}

#[test]
fn a_token_filter_only_reserves_a_name_when_that_operation_is_attempted() {
    let mut l = language("brace_mold");
    l.grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap()
        .application[0]
        .excluded_receivers = serde_json::from_value(
        json!([{"kinds":["identifier"],"token":{"text":"special","kind":"identifier"}}]),
    )
    .unwrap();
    let plain = analyze(l.clone(), "VAR special; VAR value=special;", "plain");
    assert!(plain.diagnostics.iter().all(|d| d.stage != "parser"));
    let rejected = analyze(
        l.clone(),
        "VAR special; VAR value=special(1); VAR after=2;",
        "reserved",
    );
    assert!(rejected.diagnostics.iter().any(|d| d.stage == "parser"));
    let member = analyze(l, "VAR obj; VAR value=obj.special(1);", "member");
    assert!(member.diagnostics.iter().all(|d| d.stage != "parser"));
}

#[test]
fn receiver_contracts_are_versioned_bounded_and_empty_compatible() {
    let original = language("brace_mold");
    for (key, value) in [
        ("kinds", json!([])),
        ("kinds", json!(["group", "group"])),
        ("operator", json!("unknown")),
        ("descendant_operators", json!(["unknown"])),
        ("token", json!({"text":"x","kind":"missing"})),
    ] {
        let mut l = original.clone();
        let mut grammar = serde_json::to_value(&l.grammar).unwrap();
        grammar["contexts"]["expression"]["expression"]["application"][0]["excluded_receivers"]
            [0][key] = value;
        l.grammar = serde_json::from_value(grammar).unwrap();
        assert!(l.validate().is_err(), "{key}");
    }
    let mut old = original;
    old.grammar.schema_version = 9;
    assert!(old.validate().is_err());
    old.grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap()
        .application[0]
        .excluded_receivers
        .clear();
    old.validate().unwrap();
    let encoded = serde_json::to_value(&old.grammar).unwrap();
    assert!(
        encoded["contexts"]["expression"]["expression"]["application"][0]
            .get("excluded_receivers")
            .is_none()
    );
    let _schema_value: Value = encoded;
}

#[test]
fn root_operators_and_other_delimited_operations_have_independent_policies() {
    let mut l = language("brace_mold");
    let expression = l
        .grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap();
    expression.application[0].excluded_receivers=serde_json::from_value(json!([{"kinds":["group"],"operator":"(","descendant_kinds":["identifier","member"],"descendant_operators":[]}])).unwrap();
    let simple = analyze(l.clone(), "VAR a; VAR value=(a)(1);", "root-op");
    assert!(simple.diagnostics.iter().any(|d| d.stage == "parser"));
    let compound = analyze(l.clone(), "VAR a; VAR value=(a.b)(1);", "desc-op");
    assert!(compound.diagnostics.iter().all(|d| d.stage != "parser"));
    let expression = l
        .grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap();
    let mut index = expression.application[0].clone();
    index.open = "[".into();
    index.close = "]".into();
    index.expression_kind = Some(K::Index);
    index.max_arguments = Some(1);
    index.excluded_receivers.clear();
    expression.application.push(index);
    l.grammar.groups.push(nexora_language::DelimiterPair {
        open: "[".into(),
        close: "]".into(),
        kind: Some("delimiter".into()),
    });
    l.validate().unwrap();
    let indexed = analyze(l, "VAR a; VAR value=(a)[0];", "independent-index");
    assert!(indexed.diagnostics.iter().all(|d| d.stage != "parser"));
    assert!(
        indexed
            .ast
            .nodes
            .iter()
            .any(|n| n.expression.as_ref().is_some_and(|s| s.kind == K::Index))
    );
    assert!(
        indexed
            .graph
            .edges
            .iter()
            .all(|e| e.kind != nexora_graph::EdgeKind::Calls)
    );
}

#[test]
fn summaries_keep_measured_application_chain_work_linear_and_compilation_budgeted() {
    let mut l = language("brace_mold");
    l.grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap()
        .application[0]
        .excluded_receivers = serde_json::from_value(
        json!([{"kinds":["application"],"descendant_kinds":["application","literal"]}]),
    )
    .unwrap();
    l.validate().unwrap();
    let lexer = nexora_lexer::Lexer::new(&l.lexer).unwrap();
    let parser = nexora_parser::Parser::new(l.grammar.clone());
    let run = |count| {
        let source = format!("f{}", "(0)".repeat(count));
        let tokens = lexer.tokenize(&source).unwrap();
        parser
            .parse_in_context_measured(&tokens, "expression", Default::default(), 1_000_000)
            .unwrap()
    };
    let small = run(20);
    let large = run(80);
    assert!(small.tree.diagnostics.is_empty() && large.tree.diagnostics.is_empty());
    assert!(
        large.steps <= small.steps * 5,
        "{} vs {}",
        small.steps,
        large.steps
    );
    let tokens = lexer.tokenize("f(0)").unwrap();
    let exhausted = parser
        .parse_in_context_measured(&tokens, "expression", Default::default(), 5)
        .unwrap();
    assert!(exhausted.budget_exhausted);
    assert!(exhausted.steps <= 5);
    let mut old = l.grammar;
    old.contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap()
        .application[0]
        .excluded_receivers
        .clear();
    old.schema_version = 9;
    let legacy = nexora_parser::Parser::new(old)
        .parse_in_context_measured(
            &lexer.tokenize(&format!("f{}", "(0)".repeat(80))).unwrap(),
            "expression",
            Default::default(),
            1_000_000,
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(legacy.tree).unwrap(),
        serde_json::to_value(large.tree).unwrap()
    );
}

#[test]
fn optional_member_operators_and_long_qualified_names_are_distinguished() {
    let mut l = language("brace_mold");
    let e = l
        .grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap();
    e.member.push(nexora_language::UnaryOperator {
        token: nexora_language::TokenSelector {
            text: "!".into(),
            kind: Some("operator".into()),
        },
        precedence: 90,
    });
    l.validate().unwrap();
    let optional = analyze(l.clone(), "VAR a; VAR value=(a!b)(1);", "different-member");
    assert!(optional.diagnostics.iter().all(|d| d.stage != "parser"));
    let chain = std::iter::repeat_n("part", 70)
        .collect::<Vec<_>>()
        .join(".");
    let source = format!("VAR part; VAR value=({chain})(1); VAR after=2;");
    let deep = analyze(l, &source, "qualified-deep");
    assert!(deep.diagnostics.iter().any(|d| d.stage == "parser"));
    assert!(deep.graph.nodes.iter().any(|n| n.name == "after"));
    assert!(deep.ast.nodes.iter().all(|n| {
        n.expression
            .as_ref()
            .is_none_or(|s| s.kind != K::Application)
    }));
}

#[test]
fn receiver_limits_and_unvalidated_parser_inputs_fail_without_panicking() {
    let base = language("brace_mold");
    for count in [16, 17] {
        let mut l = base.clone();
        let e = l
            .grammar
            .contexts
            .get_mut("expression")
            .unwrap()
            .expression
            .as_mut()
            .unwrap();
        let filter = e.application[0].excluded_receivers[0].clone();
        e.application[0].excluded_receivers = vec![filter; count];
        assert_eq!(l.validate().is_ok(), count == 16);
    }
    let lexer = nexora_lexer::Lexer::new(&base.lexer).unwrap();
    let tokens = lexer.tokenize("(a)(1)").unwrap();
    for malformed in [
        json!([{"kinds":[]}]),
        json!([{"kinds":["group"],"operator":"missing"}]),
        json!([{"kinds":["group"],"descendant_operators":[".","."]}]),
    ] {
        let mut l = base.clone();
        l.grammar
            .contexts
            .get_mut("expression")
            .unwrap()
            .expression
            .as_mut()
            .unwrap()
            .application[0]
            .excluded_receivers = serde_json::from_value(malformed).unwrap();
        let measured = nexora_parser::Parser::new(l.grammar)
            .parse_in_context_measured(&tokens, "expression", Default::default(), 100_000)
            .unwrap();
        assert!(!measured.tree.diagnostics.is_empty());
        assert!(measured.tree.nodes.iter().all(|n| {
            n.expression
                .as_ref()
                .is_none_or(|s| s.kind != K::Application)
        }));
    }
}

#[test]
fn context_filter_total_has_an_independent_limit() {
    for total in [64, 65] {
        let mut l = language("brace_mold");
        let e = l
            .grammar
            .contexts
            .get_mut("expression")
            .unwrap()
            .expression
            .as_mut()
            .unwrap();
        let prototype = e.application[0].clone();
        e.application.clear();
        for slot in 0..5 {
            let mut application = prototype.clone();
            application.open = format!("open{slot}");
            application.close = format!("close{slot}");
            let count = if slot < 4 { 16 } else { total - 64 };
            application.excluded_receivers = vec![prototype.excluded_receivers[0].clone(); count];
            l.grammar.groups.push(nexora_language::DelimiterPair {
                open: application.open.clone(),
                close: application.close.clone(),
                kind: application.kind.clone(),
            });
            e.application.push(application);
        }
        assert_eq!(l.validate().is_ok(), total == 64, "total {total}");
    }
}

#[test]
fn unicode_receiver_tokens_are_exact_and_parser_runs_have_no_shared_state() {
    let mut l = language("brace_mold");
    l.grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap()
        .application[0]
        .excluded_receivers = serde_json::from_value(
        json!([{"kinds":["identifier"],"token":{"text":"árbol","kind":"identifier"}}]),
    )
    .unwrap();
    l.validate().unwrap();
    let lexer = nexora_lexer::Lexer::new(&l.lexer).unwrap();
    let parser = nexora_parser::Parser::new(l.grammar);
    for (source, blocked) in [
        ("árbol(1)", true),
        ("árboles(1)", false),
        ("obj.árbol(1)", false),
    ] {
        let tokens = lexer.tokenize(source).unwrap();
        let first = parser
            .parse_in_context_measured(&tokens, "expression", Default::default(), 100_000)
            .unwrap();
        let second = parser
            .parse_in_context_measured(&tokens, "expression", Default::default(), 100_000)
            .unwrap();
        assert_eq!(!first.tree.diagnostics.is_empty(), blocked, "{source}");
        assert_eq!(first.steps, second.steps);
        assert_eq!(
            serde_json::to_value(first.tree).unwrap(),
            serde_json::to_value(second.tree).unwrap()
        );
    }
}

#[test]
fn every_compiled_operator_is_validated_even_when_no_filter_names_it() {
    let mut l = language("brace_mold");
    let group = nexora_language::DelimiterPair {
        open: "λ".repeat(65),
        close: "μ".repeat(65),
        kind: Some("delimiter".into()),
    };
    l.grammar.groups.push(group.clone());
    l.grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap()
        .grouping
        .push(group);
    assert!(
        l.validate().is_err(),
        "accepted a compiled operator exceeding 128 bytes"
    );
}
