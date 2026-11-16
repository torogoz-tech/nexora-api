use nexora_graph::{EdgeKind, NodeKind, ResolutionState};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use std::path::Path;
static NEXT_SOURCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn analyze(id: &str, source: &str) -> nexora_indexer::AnalysisResult {
    let language =
        LanguageDefinition::load(&Path::new("../../templates/web-understanding").join(id)).unwrap();
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "nexora-web-body-{}-{id}-{}",
        std::process::id(),
        NEXT_SOURCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join(format!("source.{}", language.manifest.extensions[0]));
    std::fs::write(&file, source).unwrap();
    let result = Indexer::with_catalog(language, vec![])
        .unwrap()
        .analyze(&file)
        .unwrap();
    std::fs::remove_dir_all(root).unwrap();
    result
}
#[test]
fn real_web_bodies_expose_nested_declarations_and_actual_expression_operands() {
    for id in ["typescript", "tsx", "jsx"] {
        let source = "export function compute(value) { const local = value * 2; function helper(x) { return x + 1; } { let nested = local + 1; helper(nested); } return helper(local) }";
        let result = analyze(id, source);
        assert!(
            result.diagnostics.iter().all(|d| d.stage != "parser"),
            "{:?}",
            result.diagnostics
        );
        for (name, kind) in [
            ("compute", NodeKind::Function),
            ("helper", NodeKind::Function),
            ("local", NodeKind::Variable),
            ("nested", NodeKind::Variable),
        ] {
            assert!(
                result
                    .graph
                    .nodes
                    .iter()
                    .any(|n| n.name == name && n.kind == kind),
                "missing {name} in {id}"
            );
        }
        assert!(result.ast.nodes.iter().any(|n| {
            n.expression
                .as_ref()
                .is_some_and(|e| e.operator.as_deref() == Some("*"))
        }));
        assert!(result.graph.edges.iter().any(|e| e.kind == EdgeKind::Calls));
        assert!(
            result.symbols.complete,
            "untyped compatible signatures and bodies now have complete syntax coverage"
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
fn native_literals_and_property_names_do_not_turn_all_keywords_into_literals() {
    let result = analyze(
        "typescript",
        "function values() { const yes = true; const empty = null; object.true; return this.value; }",
    );
    assert!(
        result.diagnostics.iter().all(|d| d.stage != "parser"),
        "{:?}",
        result.diagnostics
    );
    assert!(result.ast.nodes.iter().any(|n| {
        n.name == "true"
            && n.expression
                .as_ref()
                .is_some_and(|e| e.kind == nexora_core::ExpressionKind::Literal)
    }));
    assert!(result.ast.nodes.iter().any(|n| {
        n.name == "true"
            && n.expression
                .as_ref()
                .is_some_and(|e| e.kind == nexora_core::ExpressionKind::Identifier)
    }));
}
#[test]
fn jsx_and_template_text_never_create_fake_body_declarations() {
    for id in ["tsx", "jsx"] {
        let result = analyze(
            id,
            "function View() { const text = `const fake = 1;`; return <Box>const forged = 2;</Box>; } const after = 3;",
        );
        assert!(result.graph.nodes.iter().any(|n| n.name == "text"));
        assert!(result.graph.nodes.iter().any(|n| n.name == "after"));
        assert!(
            !result
                .graph
                .nodes
                .iter()
                .any(|n| n.name == "fake" || n.name == "forged")
        );
        assert!(!result.symbols.complete);
    }
}
#[test]
fn unsupported_object_arrow_and_type_details_stay_partial_or_opaque() {
    let result = analyze(
        "typescript",
        "function f(value: number): number { const item = { value: value }; return value; }",
    );
    assert!(!result.symbols.complete);
    assert!(
        result
            .graph
            .syntax_coverage
            .values()
            .any(|c| c.partial_regions > 0 && c.opaque_regions > 0)
    );
    assert!(result.graph.nodes.iter().any(|n| n.name == "item"));
}

#[test]
fn unsupported_function_scoped_var_arrows_are_not_declared_as_block_locals() {
    let result = analyze(
        "typescript",
        "function outer() { { var leaked = () => 1; } return 1; }",
    );
    assert!(
        !result.graph.nodes.iter().any(
            |n| n.name == "leaked" && matches!(n.kind, NodeKind::Function | NodeKind::Variable)
        )
    );
    assert!(
        result
            .graph
            .syntax_coverage
            .values()
            .any(|c| c.partial_regions > 0)
    );
}

#[test]
fn a_returned_regexp_does_not_erase_the_function_or_following_declaration() {
    for id in ["typescript", "tsx", "jsx"] {
        let result = analyze(
            id,
            "function matcher() { return /abc/.test(value); } const after = 1;",
        );
        assert!(
            result.diagnostics.iter().all(|d| d.stage != "parser"),
            "{:?}",
            result.diagnostics
        );
        assert!(result.graph.nodes.iter().any(|n| n.name == "matcher"));
        assert!(result.graph.nodes.iter().any(|n| n.name == "after"));
    }
}

#[test]
fn negated_regex_literals_work_at_root_and_inside_embedded_expression_modes() {
    for id in ["typescript", "tsx", "jsx"] {
        let source = "const valid = !/x[}]/.test(value); const text = `${!!/}/.test(value)}`; const after = 1;";
        let result = analyze(id, source);
        assert!(
            result.diagnostics.iter().all(|d| d.stage != "parser"),
            "{:?}",
            result.diagnostics
        );
        assert!(result.graph.nodes.iter().any(|n| n.name == "after"));
        assert_eq!(result.embedded_units.len(), 1);
        assert_eq!(
            result.embedded_units[0].status,
            nexora_core::RegionStatus::Parsed
        );
        assert!(result.graph.nodes.iter().any(|n| {
            n.expression
                .as_ref()
                .is_some_and(|e| e.operator.as_deref() == Some("!"))
        }));
    }
}

#[test]
fn prefix_negation_role_preserves_longer_inequality_operators() {
    for id in ["typescript", "tsx", "jsx"] {
        let result = analyze(
            id,
            "const different = a !== b; const text = `${a != b}`; const negated = !!/x/.test(a);",
        );
        assert!(
            result.diagnostics.iter().all(|d| d.stage != "parser"),
            "{:?}",
            result.diagnostics
        );
        assert!(result.ast.nodes.iter().any(|n| {
            n.expression
                .as_ref()
                .is_some_and(|e| e.operator.as_deref() == Some("!=="))
        }));
        assert!(result.embedded_units[0].ast.nodes.iter().any(|n| {
            n.expression
                .as_ref()
                .is_some_and(|e| e.operator.as_deref() == Some("!="))
        }));
    }
}
