use nexora_corpus::CorpusFactExtractor;
use nexora_indexer::CompositionAnalyzer;
use nexora_language::LanguageDefinition;
use std::{collections::BTreeMap, path::Path};
#[test]
fn corpus_records_unconsumed_lookahead_and_checks_original_unicode_positions() {
    let root = Path::new("../../templates/dotnet-understanding");
    let model = LanguageDefinition::load(&root.join("razor")).unwrap();
    let child = LanguageDefinition::load(&root.join("csharp")).unwrap();
    let source = "@Nameé; tail";
    let result = CompositionAnalyzer::new(model, vec![child])
        .unwrap()
        .analyze_source("source.razor", source)
        .unwrap();
    let sources = BTreeMap::from([("source.razor".to_owned(), source.to_owned())]);
    let (facts, invalid) = CorpusFactExtractor::extract(&result.graph, &sources);
    assert_eq!(invalid, 0);
    let fact = facts
        .iter()
        .map(|f| serde_json::to_value(f).unwrap())
        .find(|v| v["mode"] == "implicit_expression")
        .unwrap();
    assert_eq!(fact["lookahead"], ";");
    assert!(fact.get("closer").is_none());
    assert_eq!(fact["source"], "@Nameé");
    let mut graph = result.graph.clone();
    let lookahead = graph
        .nodes
        .iter_mut()
        .filter_map(|n| n.lexical_region.as_mut())
        .find_map(|r| r.lookahead_span.as_mut())
        .unwrap();
    lookahead.start.column += 1;
    assert!(CorpusFactExtractor::extract(&graph, &sources).1 > 0);
}

#[test]
fn virtual_unit_containment_cannot_be_forged_by_changing_registry_metadata() {
    let root = Path::new("../../templates/dotnet-understanding");
    let model = LanguageDefinition::load(&root.join("razor")).unwrap();
    let child = LanguageDefinition::load(&root.join("csharp")).unwrap();
    let source = "@Nameé; tail";
    let result = CompositionAnalyzer::new(model, vec![child])
        .unwrap()
        .analyze_source("source.razor", source)
        .unwrap();
    let sources = BTreeMap::from([("source.razor".to_owned(), source.to_owned())]);
    let id = result
        .graph
        .nodes
        .iter()
        .find(|n| n.kind == nexora_graph::NodeKind::SyntaxUnit)
        .unwrap()
        .id
        .clone();
    for change in 0..5 {
        let mut graph = result.graph.clone();
        let unit = graph.syntax_units.get_mut(&id).unwrap();
        match change {
            0 => unit.path = "missing.razor".into(),
            1 => unit.span.start.offset += 1,
            2 => unit.span.end.offset -= 1,
            3 => unit.span.start.column += 1,
            _ => {
                graph
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == id)
                    .unwrap()
                    .syntax_unit = None
            }
        }
        assert!(
            CorpusFactExtractor::extract(&graph, &sources).1 > 0,
            "mutation {change}"
        );
    }
}
