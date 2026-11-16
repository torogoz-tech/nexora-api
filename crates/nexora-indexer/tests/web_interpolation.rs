use nexora_core::RegionStatus;
use nexora_graph::{EdgeKind, NodeKind, ResolutionState};
use nexora_indexer::{CompositionAnalyzer, CompositionResult};
use nexora_language::LanguageDefinition;
use std::path::Path;

fn analyze(id: &str, source: &str) -> CompositionResult {
    let language =
        LanguageDefinition::load(&Path::new("../../templates/web-understanding").join(id)).unwrap();
    CompositionAnalyzer::new(language, vec![])
        .unwrap()
        .analyze_source(&format!("source.{id}"), source)
        .unwrap()
}

fn interiors<'a>(result: &CompositionResult, source: &'a str) -> Vec<&'a str> {
    result
        .units
        .iter()
        .filter(|u| u.context.as_deref() == Some("expression"))
        .map(|u| &source[u.span.start.offset..u.span.end.offset])
        .collect()
}

#[test]
fn real_template_interpolation_exposes_operands_with_exact_source_provenance() {
    for id in ["typescript", "tsx", "jsx"] {
        let source = "const message = `café ${format(value + 2)}!`; const after = 3;";
        let result = analyze(id, source);
        assert_eq!(interiors(&result, source), ["format(value + 2)"]);
        let unit = &result.units[1];
        assert_eq!(unit.language.0, id);
        assert_eq!(unit.status, RegionStatus::Parsed);
        assert!(unit.ast.nodes.iter().any(|n| n.name == "value"));
        assert!(unit.ast.nodes.iter().any(|n| {
            n.expression
                .as_ref()
                .is_some_and(|e| e.operator.as_deref() == Some("+"))
        }));
        assert_eq!(result.interpretations.len(), 1);
        assert_eq!(result.interpretations[0].target_unit, unit.id);
        assert!(
            result
                .graph
                .edges
                .iter()
                .any(|e| e.kind == EdgeKind::Interprets)
        );
        assert!(
            result
                .graph
                .nodes
                .iter()
                .any(|n| n.name == "after" && n.kind == NodeKind::Variable)
        );
        assert!(
            !result
                .graph
                .edges
                .iter()
                .any(|e| e.kind == EdgeKind::Calls && e.resolution == ResolutionState::Resolved)
        );
    }
}

#[test]
fn nested_jsx_and_templates_retain_each_expression_without_parsing_literal_text() {
    for id in ["tsx", "jsx"] {
        let source = "function View() { return <Box title={label(value)} text=\"const forged = 1;\">const fake = 2;{`hello ${render(item)}`}</Box>; } const after = 3;";
        let result = analyze(id, source);
        let mut actual = interiors(&result, source);
        actual.sort();
        assert_eq!(
            actual,
            ["`hello ${render(item)}`", "label(value)", "render(item)"]
        );
        assert!(
            result
                .units
                .iter()
                .skip(1)
                .all(|u| u.status == RegionStatus::Parsed)
        );
        assert!(!result.graph.nodes.iter().any(|n| matches!(
            n.kind,
            NodeKind::Variable | NodeKind::Function
        )
            && ["fake", "forged"].contains(&n.name.as_str())));
        assert!(result.graph.nodes.iter().any(|n| n.name == "after"));
        assert!(
            !result.complete,
            "JSX headers and signatures remain unknown"
        );
    }
}

#[test]
fn unsupported_interpolated_syntax_remains_partial_and_keeps_sibling_units() {
    let source = "const text = `${items.map(x => x)} ${value + 1}`;";
    let result = analyze("typescript", source);
    assert_eq!(
        interiors(&result, source),
        ["items.map(x => x)", "value + 1"]
    );
    assert_eq!(result.units[1].status, RegionStatus::Partial);
    assert_eq!(result.units[2].status, RegionStatus::Parsed);
    assert!(!result.complete);
}

#[test]
fn an_unclosed_interpolation_never_becomes_complete() {
    let source = "const text = `hello ${value + 1";
    let result = analyze("typescript", source);
    assert_eq!(interiors(&result, source), ["value + 1"]);
    assert!(!result.complete);
    assert!(
        result
            .interpretations
            .iter()
            .all(|link| !link.boundary_closed && link.status != RegionStatus::Parsed)
    );
}

#[test]
fn internal_object_and_block_braces_do_not_create_spurious_interpolations() {
    for (id, source, expected) in [
        ("typescript", "const text = `${{value}}`;", "{value}"),
        ("tsx", "const view = <Box>{({value})}</Box>;", "({value})"),
        (
            "jsx",
            "const view = <Box>{(() => { return value; })()}</Box>;",
            "(() => { return value; })()",
        ),
    ] {
        let result = analyze(id, source);
        assert_eq!(interiors(&result, source), [expected]);
        assert!(!result.complete);
        assert_eq!(result.units[1].status, RegionStatus::Partial);
    }
}

#[test]
fn dense_real_interpolation_files_are_limited_by_actual_work_not_grammar_reservations() {
    let source = format!("const text = `{}`;", "${value + 1}".repeat(80));
    let result = analyze("typescript", &source);
    assert_eq!(result.units.len(), 81);
    assert_eq!(result.interpretations.len(), 80);
    assert!(
        result
            .units
            .iter()
            .skip(1)
            .all(|u| u.status == RegionStatus::Parsed)
    );
}

#[test]
fn shared_operation_budget_accounts_actual_work_across_real_interpolations() {
    use nexora_indexer::CompositionLimits;
    let language =
        LanguageDefinition::load(Path::new("../../templates/web-understanding/typescript"))
            .unwrap();
    let source = "const text = `${value + 1} ${other + 2}`;";
    let analyzer = CompositionAnalyzer::with_limits(
        language.clone(),
        vec![],
        CompositionLimits {
            max_parser_steps: 5_000,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        analyzer
            .analyze_source("source.ts", source)
            .unwrap()
            .units
            .len(),
        3
    );
    let analyzer = CompositionAnalyzer::with_limits(
        language,
        vec![],
        CompositionLimits {
            max_parser_steps: 2,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        analyzer.analyze_source("source.ts", source),
        Err(nexora_core::NexoraError::Limit(_))
    ));
}

#[test]
fn small_per_unit_parses_cannot_reset_the_shared_file_operation_allowance() {
    use nexora_indexer::CompositionLimits;
    let language =
        LanguageDefinition::load(Path::new("../../templates/web-understanding/typescript"))
            .unwrap();
    let analyzer = CompositionAnalyzer::with_limits(
        language,
        vec![],
        CompositionLimits {
            max_parser_steps: 5_000,
            ..Default::default()
        },
    )
    .unwrap();
    let expression = "${a + b + c + d + e + f + g + h}";
    let one = format!("const text = `{expression}`;");
    assert_eq!(
        analyzer.analyze_source("one.ts", &one).unwrap().units.len(),
        2
    );
    let many = format!("const text = `{}`;", expression.repeat(80));
    assert!(matches!(
        analyzer.analyze_source("many.ts", &many),
        Err(nexora_core::NexoraError::Limit(_))
    ));
}
