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

#[test]
fn real_function_and_arrow_parameters_are_declarations_in_their_function_scope() {
    for id in ["typescript", "tsx", "jsx"] {
        for source in [
            "function render(value, count) { return value + count; }",
            "const render = (value, count) => value + count;",
            "const render = value => value + 1;",
            "const render = async (value) => value;",
        ] {
            let result = analyze(id, source);
            let parameters: Vec<_> = result
                .graph
                .nodes
                .iter()
                .filter(|n| n.kind == NodeKind::Parameter)
                .collect();
            assert!(!parameters.is_empty(), "missing parameters: {id} {source}");
            let function = result
                .graph
                .nodes
                .iter()
                .find(|n| n.kind == NodeKind::Function && n.name == "render")
                .unwrap();
            assert!(
                parameters
                    .iter()
                    .all(|n| n.scope_id == function.owned_scope_id)
            );
            assert!(result.complete, "{:?}", result.units[0].diagnostics);
            assert!(result.graph.edges.iter().any(
                |e| e.kind == EdgeKind::References && e.resolution == ResolutionState::Resolved
            ));
            assert!(!result.graph.edges.iter().any(|e| e.kind == EdgeKind::Calls && e.resolution == ResolutionState::Resolved));
        }
    }
}

#[test]
fn embedded_reads_bind_to_the_parameter_of_the_actual_containing_function() {
    let source = "function outer(value) { function inner(value) { return `${value + 1}`; } return `${value + 2}`; }";
    let result = analyze("typescript", source);
    assert_eq!(result.units.len(), 3);
    assert!(result.complete);
    for (text, owner_name) in [("value + 1", "inner"), ("value + 2", "outer")] {
        let unit = result
            .units
            .iter()
            .find(|u| &source[u.span.start.offset..u.span.end.offset] == text)
            .unwrap();
        let owner = result
            .graph
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Function && n.name == owner_name)
            .unwrap();
        let parameter = result
            .graph
            .nodes
            .iter()
            .find(|n| n.kind == NodeKind::Parameter && n.scope_id == owner.owned_scope_id)
            .unwrap();
        let reference = result
            .graph
            .edges
            .iter()
            .find(|e| {
                e.kind == EdgeKind::References
                    && e.evidence
                        .syntax_unit
                        .as_ref()
                        .and_then(|id| result.graph.syntax_units.get(id))
                        .is_some_and(|p| p.unit_id == unit.id)
                    && e.scope_binding.as_ref().is_some_and(|b| b.name == "value")
            })
            .unwrap();
        assert_eq!(reference.resolution, ResolutionState::Resolved);
        assert_eq!(reference.target, parameter.id);
    }
}

#[test]
fn block_shadowing_does_not_bind_a_temporal_dead_zone_read_to_an_outer_parameter() {
    let result = analyze(
        "typescript",
        "function render(value) { { const text = `${value}`; let value = 1; } }",
    );
    let inner = result
        .graph
        .nodes
        .iter()
        .find(|n| n.kind == NodeKind::Variable && n.name == "value")
        .unwrap();
    let reference = result
        .graph
        .edges
        .iter()
        .find(|e| {
            e.kind == EdgeKind::References
                && e.scope_binding.as_ref().is_some_and(|b| b.name == "value")
        })
        .unwrap();
    assert_eq!(reference.resolution, ResolutionState::Resolved);
    assert_eq!(reference.target, inner.id);
}

#[test]
fn typed_parameters_preserve_unknown_annotations_without_creating_type_named_parameters() {
    let result = analyze(
        "typescript",
        "function render(value: Domain.Item[], optional?: number): string { return `${value}`; }",
    );
    let names: Vec<_> = result
        .graph
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Parameter)
        .map(|n| n.name.as_str())
        .collect();
    assert_eq!(names, ["value", "optional"]);
    assert!(!result.complete);
    assert!(
        !result
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Calls && e.resolution == ResolutionState::Resolved)
    );
}

#[test]
fn unsupported_multiline_generic_parameter_types_do_not_invent_parameters_from_type_arguments() {
    let result = analyze(
        "typescript",
        "function render(value: Map<A,\n B,\n C>) { return value; }",
    );
    assert!(!result.complete);
    assert!(
        !result
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Parameter && ["A", "B", "C"].contains(&n.name.as_str()))
    );
}

#[test]
fn known_bindings_do_not_certify_call_execution_or_unknown_types() {
    let result = analyze(
        "typescript",
        "function helper(x) { return x; } function render(value) { helper(value); return `${helper(value)}`; }",
    );
    assert!(result.complete);
    let calls: Vec<_> = result
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .collect();
    assert_eq!(calls.len(), 2);
    assert!(
        calls
            .iter()
            .all(|e| e.resolution == ResolutionState::Unresolved)
    );
    let typed = analyze(
        "typescript",
        "function render(value: number) { return `${value}`; }",
    );
    assert!(!typed.complete);
    assert!(
        !typed
            .graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::References && e.resolution == ResolutionState::Resolved)
    );
}

#[test]
fn unsupported_or_malformed_signatures_stay_partial_without_forged_parameters() {
    for source in [
        "function render(value = 1) { return value; }",
        "function render({ value }) { return value; }",
        "function render(value other) { return value; }",
        "function render(value,\n, forged) { return value; }",
        "function render<T>(value: T) { return value; }",
    ] {
        let result = analyze("typescript", source);
        assert!(!result.complete, "{source}");
        assert!(
            !result.graph.nodes.iter().any(
                |n| n.kind == NodeKind::Parameter && ["forged", "T"].contains(&n.name.as_str())
            )
        );
        assert!(
            result
                .graph
                .edges
                .iter()
                .filter(|e| e.kind == EdgeKind::Calls)
                .all(|e| e.resolution == ResolutionState::Unresolved)
        );
    }
}

#[test]
fn arrow_signatures_with_restricted_line_breaks_never_publish_confident_bindings() {
    for id in ["typescript", "tsx", "jsx"] {
        for gap in ["\n", "\r", "\u{2028}", "\u{2029}", " /*\n*/ "] {
            for source in [
                format!("const render = (value){gap}=> value;"),
                format!("const render = async{gap}(value) => value;"),
            ] {
                let result = analyze(id, &source);
                assert!(!result.complete, "{id}: {source:?}");
                assert!(
                    !result
                        .graph
                        .edges
                        .iter()
                        .any(|e| e.kind == EdgeKind::References
                            && e.resolution == ResolutionState::Resolved)
                );
            }
        }
        let valid = analyze(
            id,
            "const render = async /* comment */ (value,\n count) => value + count;",
        );
        assert!(valid.complete);
    }
}
