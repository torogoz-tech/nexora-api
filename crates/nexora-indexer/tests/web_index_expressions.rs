use nexora_indexer::CompositionAnalyzer;
use nexora_language::LanguageDefinition;
use std::path::Path;

#[test]
fn web_models_interpret_indices_without_call_edges() {
    for (language, source) in [
        (
            "typescript",
            "function pick(items, index) { return items[index]; }",
        ),
        ("tsx", "const View = (props) => <p>{props.items[0]}</p>;"),
        ("jsx", "const View = (props) => <p>{props.items[0]}</p>;"),
    ] {
        let profile = LanguageDefinition::load(
            &Path::new("../../templates/web-understanding").join(language),
        )
        .unwrap();
        let result = CompositionAnalyzer::new(profile, vec![])
            .unwrap()
            .analyze_source(&format!("public.{language}"), source)
            .unwrap();
        assert!(
            result.units.iter().flat_map(|u| &u.ast.nodes).any(|n| n
                .expression
                .as_ref()
                .is_some_and(|s| s.kind == nexora_core::ExpressionKind::Index)),
            "{language}"
        );
        assert!(
            result
                .graph
                .edges
                .iter()
                .all(|e| e.kind != nexora_graph::EdgeKind::Calls)
        );
    }
}

#[test]
fn unsupported_comma_index_does_not_become_multidimensional_or_hide_neighbors() {
    let source = "function pick(items) { return items[0,1]; } const after=1;";
    for language in ["typescript", "tsx", "jsx"] {
        let profile = LanguageDefinition::load(
            &Path::new("../../templates/web-understanding").join(language),
        )
        .unwrap();
        let result = CompositionAnalyzer::new(profile, vec![])
            .unwrap()
            .analyze_source(&format!("public.{language}"), source)
            .unwrap();
        assert!(
            result
                .units
                .iter()
                .any(|u| u.status == nexora_core::RegionStatus::Partial)
        );
        assert!(result.units.iter().flat_map(|u| &u.ast.nodes).all(|n| {
            n.expression
                .as_ref()
                .is_none_or(|s| s.kind != nexora_core::ExpressionKind::Index)
        }));
        assert!(
            result
                .graph
                .nodes
                .iter()
                .any(|n| n.kind == nexora_graph::NodeKind::Variable && n.name == "after")
        );
    }
}
