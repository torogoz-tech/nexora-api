use nexora_core::{ExpressionKind, RegionStatus};
use nexora_indexer::CompositionAnalyzer;
use nexora_language::LanguageDefinition;
use std::path::Path;

#[test]
fn web_models_preserve_conditional_arms_and_nested_assignment_expressions() {
    for language in ["typescript", "tsx", "jsx"] {
        let profile = LanguageDefinition::load(
            &Path::new("../../templates/web-understanding").join(language),
        )
        .unwrap();
        let source = "function pick(flag, a, b) { return flag ? a=1 : b ? a : b=2; }";
        let result = CompositionAnalyzer::new(profile, vec![])
            .unwrap()
            .analyze_source(&format!("public.{language}"), source)
            .unwrap();
        let nodes: Vec<_> = result.units.iter().flat_map(|u| &u.ast.nodes).collect();
        let conditionals: Vec<_> = nodes
            .iter()
            .filter(|n| {
                n.expression
                    .as_ref()
                    .is_some_and(|s| s.kind == ExpressionKind::Conditional)
            })
            .collect();
        assert_eq!(conditionals.len(), 2, "{language}");
        for node in conditionals {
            assert_eq!(node.expression.as_ref().unwrap().operands.len(), 3);
        }
        assert!(
            result
                .units
                .iter()
                .all(|u| u.status != RegionStatus::Partial),
            "{language}"
        );
        let encoded = serde_json::to_vec(&result.graph).unwrap();
        let _: nexora_graph::CodeGraph = serde_json::from_slice(&encoded).unwrap();
    }
}

#[test]
fn jsx_branch_carriers_keep_their_embedded_expressions() {
    for language in ["tsx", "jsx"] {
        let profile = LanguageDefinition::load(
            &Path::new("../../templates/web-understanding").join(language),
        )
        .unwrap();
        let source =
            "const View = (props) => props.ready ? <p>{props.items[0]}</p> : <b>{props.label}</b>;";
        let result = CompositionAnalyzer::new(profile, vec![])
            .unwrap()
            .analyze_source(&format!("public.{language}"), source)
            .unwrap();
        let nodes: Vec<_> = result.units.iter().flat_map(|u| &u.ast.nodes).collect();
        assert_eq!(
            nodes
                .iter()
                .filter(|n| n
                    .expression
                    .as_ref()
                    .is_some_and(|s| s.kind == ExpressionKind::Conditional))
                .count(),
            1,
            "{language}"
        );
        assert!(
            nodes.iter().any(|n| n
                .expression
                .as_ref()
                .is_some_and(|s| s.kind == ExpressionKind::Index)),
            "{language}"
        );
        assert!(
            nodes
                .iter()
                .any(|n| &source[n.span.start.offset..n.span.end.offset] == "props.label"),
            "{language}"
        );
        assert!(
            result
                .units
                .iter()
                .all(|u| u.status != RegionStatus::Partial),
            "{language}"
        );
    }
}

#[test]
fn malformed_conditional_preserves_following_web_declarations() {
    for language in ["typescript", "tsx", "jsx"] {
        for invalid in ["flag ? : other", "flag ? value :", "flag ? value"] {
            let profile = LanguageDefinition::load(
                &Path::new("../../templates/web-understanding").join(language),
            )
            .unwrap();
            let result = CompositionAnalyzer::new(profile, vec![])
                .unwrap()
                .analyze_source(
                    &format!("public.{language}"),
                    &format!("const value={invalid}; const after=1;"),
                )
                .unwrap();
            assert!(
                result
                    .units
                    .iter()
                    .any(|u| u.status == RegionStatus::Partial),
                "{language}: {invalid}"
            );
            assert!(
                result
                    .graph
                    .nodes
                    .iter()
                    .any(|n| n.name == "after" && n.kind == nexora_graph::NodeKind::Variable),
                "{language}: {invalid}"
            );
        }
    }
}
