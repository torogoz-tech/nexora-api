use nexora_indexer::CompositionAnalyzer;
use nexora_language::LanguageDefinition;
use std::path::Path;
fn profiles(selector: Option<&str>) -> (LanguageDefinition, LanguageDefinition) {
    let mut host =
        LanguageDefinition::load(Path::new("../../templates/semantic-molds/brace_mold")).unwrap();
    let mut target = host.clone();
    target.manifest.id.0 = "entry_target".into();
    let mut lexer = serde_json::to_value(&host.lexer).unwrap();
    lexer["schema_version"] = 4.into();
    lexer["tokens"].as_array_mut().unwrap().insert(
        0,
        serde_json::json!({"kind":"island","pattern":"~\\{","region":"island"}),
    );
    lexer["regions"] = serde_json::json!({"island":{"close":"}~"}});
    host.lexer = serde_json::from_value(lexer).unwrap();
    host.grammar
        .ignored
        .push(serde_json::from_value(serde_json::json!({"type":"token","kind":"island"})).unwrap());
    host.graph.schema_version = 10;
    host.graph
        .mappings
        .insert("lexical_region".into(), "LexicalRegion".into());
    let mut manifest = serde_json::to_value(&host.manifest).unwrap();
    let mut route = serde_json::json!({"language":"entry_target","context":"expression","scope":{"visibility":"whole_scope","parent_lookup":false,"shadowing":"nearest"}});
    if let Some(selector) = selector {
        route["lexical_context"] = selector.into();
    }
    manifest["composition"] = serde_json::json!({"schema_version":6,"roles":{"island":"grammar"},"routes":{"island":route}});
    host.manifest = serde_json::from_value(manifest).unwrap();
    let mut lexer = serde_json::to_value(&target.lexer).unwrap();
    lexer["schema_version"] = 13.into();
    lexer["entry_contexts"] =
        serde_json::json!({"words":["identifier"],"alias":["identifier"],"numbers":["number"]});
    target.lexer = serde_json::from_value(lexer).unwrap();
    (host, target)
}
#[test]
fn named_lexical_views_are_part_of_unit_identity_without_changing_grammar_or_source() {
    let mut ids = vec![];
    for selected in [None, Some("words"), Some("alias")] {
        let (host, target) = profiles(selected);
        let result = CompositionAnalyzer::new(host, vec![target])
            .unwrap()
            .analyze_source("public.nxb", "~{alpha}~")
            .unwrap();
        assert_eq!(result.units.len(), 2);
        let unit = &result.units[1];
        assert_eq!(unit.context.as_deref(), Some("expression"));
        assert_eq!(unit.lexical_context.as_deref(), selected);
        assert_eq!(unit.tokens[0].text, "alpha");
        ids.push(unit.id.clone());
        let owner = result
            .graph
            .syntax_units
            .values()
            .find(|p| p.unit_id == unit.id)
            .unwrap();
        assert_eq!(owner.lexical_context.as_deref(), selected);
        nexora_graph::GraphIntegrityValidator::validate(&result.graph).unwrap();
    }
    assert_ne!(ids[0], ids[1]);
    assert_ne!(ids[1], ids[2]);
}
#[test]
fn failed_lexical_view_keeps_its_selection_and_partial_provenance() {
    let (host, target) = profiles(Some("numbers"));
    let result = CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("public.nxb", "~{alpha}~")
        .unwrap();
    assert_eq!(result.units[1].status, nexora_core::RegionStatus::Partial);
    assert_eq!(result.units[1].lexical_context.as_deref(), Some("numbers"));
    assert!(result.units[1].tokens.is_empty());
    assert!(
        result.units[1]
            .diagnostics
            .iter()
            .any(|d| d.stage == "lexer")
    );
    nexora_graph::GraphIntegrityValidator::validate(&result.graph).unwrap();
}
#[test]
fn graph_roundtrip_keeps_selection_and_rejects_inconsistent_interpretation_proof() {
    let (host, target) = profiles(Some("words"));
    let result = CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("public.nxb", "~{alpha}~")
        .unwrap();
    let graph: nexora_graph::CodeGraph =
        serde_json::from_value(serde_json::to_value(&result.graph).unwrap()).unwrap();
    nexora_graph::GraphIntegrityValidator::validate(&graph).unwrap();
    for selected in [Some("alias"), None, Some("bad/name")] {
        let mut altered = graph.clone();
        altered
            .syntax_units
            .values_mut()
            .find(|p| p.lexical_context.is_some())
            .unwrap()
            .lexical_context = selected.map(String::from);
        assert!(nexora_graph::GraphIntegrityValidator::validate(&altered).is_err());
    }
    for selected in [Some("alias"), None] {
        let mut altered = graph.clone();
        altered
            .syntax_units
            .values_mut()
            .find(|p| p.lexical_context.is_some())
            .unwrap()
            .lexical_context = selected.map(String::from);
        altered
            .edges
            .iter_mut()
            .find(|e| e.kind == nexora_graph::EdgeKind::Interprets)
            .unwrap()
            .resolution_reason = Some(selected.map_or_else(
            || "declared_embedded_context".into(),
            |name| format!("declared_embedded_context/lexical:{name}"),
        ));
        assert!(nexora_graph::GraphIntegrityValidator::validate(&altered).is_err());
    }
    let mut renamed = graph.clone();
    let owner = renamed
        .syntax_units
        .iter()
        .find(|(_, unit)| unit.lexical_context.is_some())
        .unwrap()
        .0
        .clone();
    let scope = renamed
        .nodes
        .iter()
        .find(|node| node.id == owner)
        .unwrap()
        .scope_policy
        .clone()
        .unwrap();
    let unit = renamed.syntax_units.get_mut(&owner).unwrap();
    unit.lexical_context = Some("alias".into());
    unit.unit_id = nexora_graph::SyntaxUnitIdentity::for_interpretation(
        unit.span,
        &unit.language.0,
        unit.context.as_deref().unwrap(),
        unit.lexical_context.as_deref(),
        &scope,
        &unit.profile_fingerprint,
    )
    .unwrap();
    renamed
        .edges
        .iter_mut()
        .find(|edge| edge.kind == nexora_graph::EdgeKind::Interprets)
        .unwrap()
        .resolution_reason = Some("declared_embedded_context/lexical:alias".into());
    assert!(nexora_graph::GraphIntegrityValidator::validate(&renamed).is_err());
    let mut downgraded = graph.clone();
    let owner = downgraded
        .syntax_units
        .iter()
        .find(|(_, unit)| unit.lexical_context.is_some())
        .unwrap()
        .0
        .clone();
    let scope = downgraded
        .nodes
        .iter()
        .find(|node| node.id == owner)
        .unwrap()
        .scope_policy
        .clone()
        .unwrap();
    let unit = downgraded.syntax_units.get_mut(&owner).unwrap();
    unit.lexical_context = None;
    unit.unit_id = nexora_graph::SyntaxUnitIdentity::for_interpretation(
        unit.span,
        &unit.language.0,
        unit.context.as_deref().unwrap(),
        None,
        &scope,
        &unit.profile_fingerprint,
    )
    .unwrap();
    downgraded
        .edges
        .iter_mut()
        .find(|edge| edge.kind == nexora_graph::EdgeKind::Interprets)
        .unwrap()
        .resolution_reason = Some("declared_embedded_context".into());
    assert!(nexora_graph::GraphIntegrityValidator::validate(&downgraded).is_err());
    let mut altered = graph;
    altered
        .edges
        .iter_mut()
        .find(|e| e.kind == nexora_graph::EdgeKind::Interprets)
        .unwrap()
        .resolution_reason = Some("declared_embedded_context/lexical:alias".into());
    assert!(nexora_graph::GraphIntegrityValidator::validate(&altered).is_err());
}
#[test]
fn target_entry_must_exist_and_route_selection_is_versioned() {
    let (mut host, target) = profiles(Some("words"));
    host.manifest.composition.as_mut().unwrap().schema_version = 5;
    assert!(host.validate().is_err());
    let (mut host, target2) = profiles(Some("words"));
    host.manifest
        .composition
        .as_mut()
        .unwrap()
        .routes
        .get_mut("island")
        .unwrap()
        .lexical_context = Some("missing".into());
    assert!(CompositionAnalyzer::new(host, vec![target2]).is_err());
    let (host, _) = profiles(None);
    assert!(CompositionAnalyzer::new(host, vec![target]).is_ok());
}
#[test]
fn lexical_selection_and_same_version_rule_edits_invalidate_cache_and_match_full() {
    use nexora_indexer::{IndexCache, Indexer, MemoryStore};
    let directory = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-lexical-entry-cache-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("public.nxb"),
        "VAR before; ~{alpha}~ VAR after;",
    )
    .unwrap();
    let (host, target) = profiles(Some("words"));
    let tenant = nexora_core::TenantId("entry-cache".into());
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let indexer = Indexer::with_catalog(host.clone(), vec![target.clone()]).unwrap();
    let cold = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    let warm = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 1);
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    let id = cold
        .snapshot
        .graph
        .syntax_units
        .values()
        .find(|u| u.lexical_context.is_some())
        .unwrap()
        .unit_id
        .clone();
    let mut host = host;
    host.manifest
        .composition
        .as_mut()
        .unwrap()
        .routes
        .get_mut("island")
        .unwrap()
        .lexical_context = Some("alias".into());
    let selected = Indexer::with_catalog(host.clone(), vec![target.clone()]).unwrap();
    let refreshed = selected
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(refreshed.reuse.analyzed_files, 1);
    assert!(refreshed.reuse.cache_reset);
    assert_ne!(
        id,
        refreshed
            .snapshot
            .graph
            .syntax_units
            .values()
            .find(|u| u.lexical_context.is_some())
            .unwrap()
            .unit_id
    );
    let full = selected
        .index(&directory, tenant.clone(), &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::to_value(&refreshed.snapshot).unwrap()
    );
    let mut target = target;
    target
        .lexer
        .entry_contexts
        .insert("alias".into(), vec!["number".into()]);
    let edited = Indexer::with_catalog(host, vec![target]).unwrap();
    let failed = edited
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert!(failed.reuse.cache_reset);
    assert_eq!(failed.reuse.cache_skipped_files, 1);
    assert!(
        failed
            .snapshot
            .graph
            .nodes
            .iter()
            .any(|n| n.name == "after" && n.kind == nexora_graph::NodeKind::Variable)
    );
    let full = edited
        .index(&directory, tenant, &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::to_value(&failed.snapshot).unwrap()
    );
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn one_composition_keeps_two_lexical_interpretations_of_the_same_range_separate() {
    let (mut host, mut target) = profiles(Some("words"));
    let mut lexer = serde_json::to_value(&host.lexer).unwrap();
    lexer["tokens"].as_array_mut().unwrap().insert(
        0,
        serde_json::json!({"kind":"wrapper","pattern":"#\\[","region":"wrapper"}),
    );
    lexer["regions"]["wrapper"] =
        serde_json::json!({"close":"]#","children":[{"pattern":"~\\{","region":"island"}]});
    host.lexer = serde_json::from_value(lexer).unwrap();
    host.grammar.ignored.push(
        serde_json::from_value(serde_json::json!({"type":"token","kind":"wrapper"})).unwrap(),
    );
    let route = serde_json::json!({"language":"entry_target","context":"wrapper_body","lexical_context":"wrapper","scope":{"visibility":"whole_scope","parent_lookup":false,"shadowing":"nearest"}});
    host.manifest
        .composition
        .as_mut()
        .unwrap()
        .roles
        .insert("wrapper".into(), nexora_core::RegionCoverageMethod::Grammar);
    host.manifest
        .composition
        .as_mut()
        .unwrap()
        .routes
        .insert("wrapper".into(), serde_json::from_value(route).unwrap());
    let mut lexer = serde_json::to_value(&target.lexer).unwrap();
    lexer["tokens"].as_array_mut().unwrap().insert(
        0,
        serde_json::json!({"kind":"island","pattern":"~\\{","region":"island","root":false}),
    );
    lexer["regions"] = serde_json::json!({"island":{"close":"}~"}});
    lexer["entry_contexts"]["wrapper"] = serde_json::json!(["island"]);
    target.lexer = serde_json::from_value(lexer).unwrap();
    let mut context = target.grammar.contexts["body"].clone();
    context
        .ignored
        .push(serde_json::from_value(serde_json::json!({"type":"token","kind":"island"})).unwrap());
    target
        .grammar
        .contexts
        .insert("wrapper_body".into(), context);
    target.graph.schema_version = 10;
    target
        .graph
        .mappings
        .insert("lexical_region".into(), "LexicalRegion".into());
    let mut manifest = serde_json::to_value(&target.manifest).unwrap();
    manifest["composition"] = serde_json::json!({"schema_version":6,"roles":{"island":"grammar"},"routes":{"island":{"language":"entry_target","context":"expression","lexical_context":"alias","scope":{"visibility":"whole_scope","parent_lookup":false,"shadowing":"nearest"}}}});
    target.manifest = serde_json::from_value(manifest).unwrap();
    let result = CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("public.nxb", "#[~{alpha}~]#")
        .unwrap();
    assert_eq!(result.units.len(), 4);
    let words = result
        .units
        .iter()
        .find(|u| u.lexical_context.as_deref() == Some("words"))
        .unwrap();
    let alias = result
        .units
        .iter()
        .find(|u| u.lexical_context.as_deref() == Some("alias"))
        .unwrap();
    assert_eq!(words.span, alias.span);
    assert_eq!(words.context, alias.context);
    assert_eq!(words.profile_fingerprint, alias.profile_fingerprint);
    assert_ne!(words.id, alias.id);
    assert_eq!(
        serde_json::to_value(&words.tokens).unwrap(),
        serde_json::to_value(&alias.tokens).unwrap()
    );
    nexora_graph::GraphIntegrityValidator::validate(&result.graph).unwrap();
}
