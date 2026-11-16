use nexora_core::{ExpressionKind, NexoraError, SourcePosition};
use nexora_language::LanguageDefinition;
use nexora_lexer::Lexer;
use nexora_parser::Parser;
use std::path::Path;

fn language() -> LanguageDefinition {
    LanguageDefinition::load(Path::new("../../templates/semantic-molds/brace_mold")).unwrap()
}

#[test]
fn bounded_expression_entry_uses_original_positions_and_selected_grammar() {
    let l = language();
    let source = "outside café\r\n${f(value + 2)} tail that is not code";
    let start = source.find("f(").unwrap();
    let end = source.find('}').unwrap();
    let tokens = Lexer::new(&l.lexer)
        .unwrap()
        .tokenize_range(source, start..end)
        .unwrap();
    assert_eq!(
        tokens.iter().map(|t| t.text.as_str()).collect::<String>(),
        "f(value + 2)"
    );
    let mut position = SourcePosition::default();
    position.advance(&source[..start]);
    for token in &tokens {
        assert_eq!(token.span.start, position);
        assert_eq!(
            &source[token.span.start.offset..token.span.end.offset],
            token.text
        );
        position.advance(&token.text);
        assert_eq!(token.span.end, position);
    }
    let parser = Parser::new(l.grammar);
    let tree = parser
        .parse_in_context(&tokens, "expression", tokens[0].span.start)
        .unwrap();
    assert!(tree.diagnostics.is_empty(), "{:?}", tree.diagnostics);
    assert_eq!(tree.context.as_deref(), Some("expression"));
    assert!(tree.file_scope.is_none());
    assert!(tree.nodes.iter().any(|n| {
        n.expression
            .as_ref()
            .is_some_and(|e| e.kind == ExpressionKind::Application)
    }));
    assert!(
        tree.nodes
            .iter()
            .all(|n| n.span.start.offset >= start && n.span.end.offset <= end)
    );
    assert!(
        !parser.parse(&tokens).diagnostics.is_empty(),
        "root grammar must not accept a bare expression"
    );
}

#[test]
fn bounded_body_uses_context_productions_and_keeps_following_host_text_out() {
    let l = language();
    let source = "<host>{VAR inner; READ inner;}<sibling>";
    let start = source.find('{').unwrap() + 1;
    let end = source.find('}').unwrap();
    let tokens = Lexer::new(&l.lexer)
        .unwrap()
        .tokenize_range(source, start..end)
        .unwrap();
    let tree = Parser::new(l.grammar)
        .parse_in_context(&tokens, "body", tokens[0].span.start)
        .unwrap();
    assert!(tree.diagnostics.is_empty(), "{:?}", tree.diagnostics);
    assert_eq!(tree.nodes.len(), 2);
    assert!(tree.nodes.iter().all(|n| n.name == "inner"));
    assert_eq!(tree.nodes.last().unwrap().span.end.offset, end);
}

#[test]
fn empty_or_missing_context_never_falls_back_to_the_root_or_panics() {
    let parser = Parser::new(language().grammar);
    assert!(matches!(
        parser.parse_in_context(&[], "missing", SourcePosition::default()),
        Err(NexoraError::Configuration(_))
    ));
    assert!(matches!(
        parser.parse_in_context(&[], "", SourcePosition::default()),
        Err(NexoraError::Configuration(_))
    ));
    let origin = SourcePosition {
        offset: 53,
        line: 4,
        column: 8,
    };
    let expression = parser.parse_in_context(&[], "expression", origin).unwrap();
    assert_eq!(expression.diagnostics[0].span.start, origin);
    assert_eq!(expression.diagnostics[0].span.end, origin);
    assert!(
        !expression.diagnostics.is_empty(),
        "an empty required expression is incomplete"
    );
    let body = parser.parse_in_context(&[], "body", origin).unwrap();
    assert!(body.nodes.is_empty() && body.diagnostics.is_empty());
}

#[test]
fn lexer_range_rejects_invalid_boundaries_and_preserves_absolute_error_offsets() {
    let lexer = Lexer::new(&language().lexer).unwrap();
    let source = "é x @";
    for (start, end) in [(1, 2), (0, 1), (4, 3), (0, 100)] {
        assert!(matches!(
            lexer.tokenize_range(source, start..end),
            Err(NexoraError::Configuration(_))
        ));
    }
    assert!(lexer.tokenize_range(source, 2..2).unwrap().is_empty());
    let error = lexer.tokenize_range(source, 5..6).unwrap_err().to_string();
    assert!(error.contains("byte 5"), "{error}");
    let source = "é\r\nVAR x;";
    let root = lexer.tokenize(source).unwrap();
    let ranged = lexer.tokenize_range(source, 0..source.len()).unwrap();
    assert_eq!(
        serde_json::to_value(root).unwrap(),
        serde_json::to_value(ranged).unwrap()
    );
}

#[test]
fn bounded_scanner_cannot_consume_a_closer_outside_the_region() {
    let l = LanguageDefinition::load(Path::new("../../languages/typescript")).unwrap();
    let source = "prefix é\r\n`hello ${value}`; const after = 1;";
    let start = source.find('`').unwrap();
    let end = source.find('}').unwrap();
    let tokens = Lexer::new(&l.lexer)
        .unwrap()
        .tokenize_range(source, start..end)
        .unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].span.start.offset, start);
    assert_eq!(tokens[0].span.end.offset, end);
    assert_eq!(tokens[0].regions.len(), 2);
    assert!(
        tokens[0]
            .regions
            .iter()
            .all(|r| !r.closed && r.span.end.offset == end)
    );
    let error = Lexer::new(&l.lexer)
        .unwrap()
        .tokenize_range("prefix 'quoted'", 7..10)
        .unwrap_err()
        .to_string();
    assert!(error.contains("byte 7"), "{error}");
}

#[test]
fn selected_context_projects_real_graph_evidence_without_host_symbols() {
    let l = language();
    let source = "VAR outside; <embedded>VAR inner; READ missing; EVAL inner + missing;</embedded> VAR after;";
    let start = source.find("VAR inner").unwrap();
    let end = source.find("</embedded>").unwrap();
    let tokens = Lexer::new(&l.lexer)
        .unwrap()
        .tokenize_range(source, start..end)
        .unwrap();
    let cst = Parser::new(l.grammar.clone())
        .parse_in_context(&tokens, "body", tokens[0].span.start)
        .unwrap();
    assert!(cst.diagnostics.is_empty(), "{:?}", cst.diagnostics);
    let ast = nexora_ast::AbstractSyntaxTree::from_cst(&cst);
    let symbols = nexora_symbols::SymbolTable::from_ast(&ast);
    assert_eq!(symbols.symbols.len(), 1);
    assert_eq!(symbols.symbols[0].name, "inner");
    let projected =
        nexora_graph::CodeGraph::project_with_symbols(&ast, &symbols, "mixed.nxb", &l, true)
            .unwrap();
    let graph = projected.graph;
    for node in &graph.nodes {
        if let Some(evidence) = &node.evidence {
            assert!(evidence.span.start.offset >= start && evidence.span.end.offset <= end);
            assert!(ast.nodes.iter().any(|n| n.id == evidence.ast_id));
            assert_eq!(evidence.rule_version, l.manifest.version);
            assert!(
                !source[evidence.span.start.offset..evidence.span.end.offset].contains("outside")
            );
        }
    }
    assert!(graph.nodes.iter().any(|n| {
        n.expression
            .as_ref()
            .is_some_and(|e| e.kind == ExpressionKind::Infix)
    }));
    assert!(
        graph
            .edges
            .iter()
            .any(|e| e.kind == nexora_graph::EdgeKind::References
                && e.resolution != nexora_graph::ResolutionState::Resolved)
    );
    assert!(
        !graph
            .nodes
            .iter()
            .any(|n| n.name == "outside" || n.name == "after")
    );
}
