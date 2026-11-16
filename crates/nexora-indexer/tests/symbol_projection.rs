use nexora_core::SyntaxRole;
use nexora_graph::{NodeKind, ResolutionState};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use std::{fs, path::Path};
fn configured() -> LanguageDefinition {
    let mut language =
        LanguageDefinition::load(Path::new("../../templates/language-pack")).unwrap();
    let mut value = serde_json::to_value(&language.graph).unwrap();
    value["schema_version"] = serde_json::json!(2);
    value["mappings"]["public_name"] = serde_json::json!("Symbol");
    value["reference_relations"]["public_name"] = serde_json::json!(["EXPORTS"]);
    value
        .as_object_mut()
        .unwrap()
        .entry("symbol_references")
        .or_insert_with(|| serde_json::json!({}));
    value["symbol_references"]["public_name"] =
        serde_json::json!({"scope":"file","target_kinds":["Function"],"label":"public"});
    language.graph = serde_json::from_value(value).unwrap();
    let mut production = language
        .grammar
        .productions
        .iter()
        .find(|p| p.projection.role == SyntaxRole::Declaration)
        .unwrap()
        .clone();
    production.name = "publication".into();
    production.pattern=serde_json::from_value(serde_json::json!({"type":"sequence","items":[{"type":"literal","text":"PUBLIC"},{"type":"capture","name":"name","item":{"type":"token","kind":"identifier"},"join":""},{"type":"literal","text":";"}]})).unwrap();
    production.projection.ast_kind = "public_name".into();
    production.projection.role = SyntaxRole::Reference;
    production.projection.name_capture = "name".into();
    language.grammar.productions.push(production);
    language
}
fn analyze(
    language: LanguageDefinition,
    source: &str,
    name: &str,
) -> nexora_indexer::AnalysisResult {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-symbol-{}-{name}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("source.nxexample");
    fs::write(&file, source).unwrap();
    let result = Indexer::new(language).analyze(&file).unwrap();
    fs::remove_dir_all(root).unwrap();
    result
}
#[test]
fn invented_syntax_resolves_forward_reference_and_preserves_public_label() {
    let result = analyze(
        configured(),
        "PUBLIC total;\nDEF total [] BEGIN FINISH\n",
        "forward",
    );
    assert!(result.diagnostics.is_empty());
    let edge = result
        .graph
        .edges
        .iter()
        .find(|e| serde_json::to_value(&e.kind).unwrap() == "EXPORTS")
        .unwrap();
    assert_eq!(edge.resolution, ResolutionState::Resolved);
    let target = result
        .graph
        .nodes
        .iter()
        .find(|n| n.id == edge.target)
        .unwrap();
    assert_eq!(target.kind, NodeKind::Function);
    assert_eq!(target.name, "total");
    assert_eq!(serde_json::to_value(edge).unwrap()["label"], "public");
    assert_eq!(
        &result.source.content[edge.evidence.span.start.offset..edge.evidence.span.end.offset],
        "PUBLIC total;"
    );
}
#[test]
fn missing_duplicate_wrong_kind_and_partial_analysis_never_claim_unique_resolution() {
    for (name, source, state) in [
        ("missing", "PUBLIC missing;", ResolutionState::Unresolved),
        (
            "duplicate",
            "DEF total [] BEGIN FINISH\nDEF total [] BEGIN FINISH\nPUBLIC total;",
            ResolutionState::Ambiguous,
        ),
        (
            "partial",
            "DEF total [] BEGIN FINISH\nunknown;\nPUBLIC total;",
            ResolutionState::Unresolved,
        ),
    ] {
        let result = analyze(configured(), source, name);
        assert!(!result.diagnostics.is_empty());
        let edge = result
            .graph
            .edges
            .iter()
            .find(|e| serde_json::to_value(&e.kind).unwrap() == "EXPORTS")
            .unwrap();
        assert_eq!(edge.resolution, state);
    }
    let mut language = configured();
    let value = serde_json::to_value(&language.graph).unwrap();
    let mut value = value;
    value["symbol_references"]["public_name"]["target_kinds"] = serde_json::json!(["Method"]);
    language.graph = serde_json::from_value(value).unwrap();
    let result = analyze(
        language,
        "DEF total [] BEGIN FINISH\nPUBLIC total;",
        "kinds",
    );
    assert!(!result.diagnostics.is_empty());
    assert!(
        result
            .graph
            .edges
            .iter()
            .filter(|e| serde_json::to_value(&e.kind).unwrap() == "EXPORTS")
            .all(|e| e.resolution == ResolutionState::Unresolved)
    );
}
#[test]
fn symbol_strategy_requires_schema_two_and_valid_reference_contract() {
    let valid = configured();
    assert!(valid.validate().is_ok());
    let original = serde_json::to_value(&valid.graph).unwrap();
    for mode in [
        "old-schema",
        "wrong-scope",
        "empty-kinds",
        "missing-strategy",
        "wrong-mapping",
        "bad-relation",
        "oversized-label",
    ] {
        let mut v = original.clone();
        match mode {
            "old-schema" => v["schema_version"] = serde_json::json!(1),
            "wrong-scope" => {
                v["symbol_references"]["public_name"]["scope"] = serde_json::json!("global")
            }
            "empty-kinds" => {
                v["symbol_references"]["public_name"]["target_kinds"] = serde_json::json!([])
            }
            "missing-strategy" => v["symbol_references"] = serde_json::json!({}),
            "wrong-mapping" => v["mappings"]["public_name"] = serde_json::json!("Module"),
            "bad-relation" => {
                v["reference_relations"]["public_name"] = serde_json::json!(["IMPORTS"])
            }
            _ => {
                v["symbol_references"]["public_name"]["label"] = serde_json::json!("a".repeat(129))
            }
        };
        let parsed = serde_json::from_value(v);
        if let Ok(graph) = parsed {
            let mut l = valid.clone();
            l.graph = graph;
            assert!(l.validate().is_err(), "{mode}");
        }
    }
}
#[test]
fn same_projection_strategy_can_emit_a_generic_reference_relation() {
    let mut language = configured();
    language
        .graph
        .reference_relations
        .insert("public_name".into(), vec!["REFERENCES".into()]);
    let result = analyze(
        language,
        "DEF total [] BEGIN FINISH\nPUBLIC total;",
        "generic-reference",
    );
    assert!(result.diagnostics.is_empty());
    assert!(
        result
            .graph
            .edges
            .iter()
            .any(|e| serde_json::to_value(&e.kind).unwrap() == "REFERENCES"
                && e.resolution == ResolutionState::Resolved)
    );
}
