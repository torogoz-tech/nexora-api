use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use std::path::Path;

#[test]
fn fixture_builds_traceable_graph() {
    let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
    let result = Indexer::new(language)
        .analyze(Path::new("../../tests/fixtures/javascript/declarations.js"))
        .unwrap();
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(
        result
            .ast
            .nodes
            .iter()
            .map(|n| n.name.as_str())
            .collect::<Vec<_>>(),
        ["greet", "Greeter"]
    );
    assert_eq!(result.graph.nodes.len(), 3);
    assert_eq!(result.graph.edges.len(), 2);
    for node in &result.ast.nodes {
        assert!(
            result.source.content[node.span.start.offset..node.span.end.offset]
                .contains(&node.name)
        );
        assert!(result.cst.nodes.iter().any(|n| n.id == node.cst_id));
    }
    assert_eq!(
        result
            .tokens
            .iter()
            .map(|t| t.text.as_str())
            .collect::<String>(),
        result.source.content
    );
}
