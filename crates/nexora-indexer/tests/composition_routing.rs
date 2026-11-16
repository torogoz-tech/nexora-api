use nexora_core::{RegionStatus, SourcePosition};
use nexora_indexer::{CompositionAnalyzer, CompositionLimits};
use nexora_language::LanguageDefinition;
use std::path::Path;

fn profiles(alternate: bool) -> (LanguageDefinition, LanguageDefinition) {
    let mut host =
        LanguageDefinition::load(Path::new("../../templates/semantic-molds/brace_mold")).unwrap();
    let mut lexer = serde_json::to_value(&host.lexer).unwrap();
    lexer["schema_version"] = 4.into();
    lexer["tokens"].as_array_mut().unwrap().insert(
        0,
        serde_json::json!({"kind":"template", "pattern":"~\\[", "region":"template"}),
    );
    lexer["regions"] = serde_json::json!({
        "template":{"close":"]~","children":[{"pattern":"@\\(","region":"code"}]},
        "code":{"close":")","children":[{"pattern":"\\(","region":"paren"},{"pattern":"~\\[","region":"template"},{"pattern":"\"","region":"quote"}]},
        "paren":{"close":")","children":[{"pattern":"\\(","region":"paren"},{"pattern":"~\\[","region":"template"},{"pattern":"\"","region":"quote"}]},
        "quote":{"close":"\"","escape":"\\"}
    });
    if alternate {
        lexer["tokens"][0]["pattern"] = "#\\[".into();
        lexer["regions"]["template"]["close"] = "]#".into();
        lexer["regions"]["template"]["children"][0]["pattern"] = "@<".into();
        lexer["regions"]["code"]["close"] = ">".into();
        for name in ["code", "paren"] {
            lexer["regions"][name]["children"][1]["pattern"] = "#\\[".into();
        }
    }
    host.lexer = serde_json::from_value(lexer).unwrap();
    host.grammar.ignored.push(
        serde_json::from_value(serde_json::json!({"type":"token","kind":"template"})).unwrap(),
    );
    host.graph.schema_version = 7;
    host.graph
        .mappings
        .insert("lexical_region".into(), "LexicalRegion".into());
    let mut target = host.clone();
    target.manifest.id.0 = "inner_mold".into();
    target
        .grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap()
        .literals
        .push("template".into());
    for profile in [&mut host, &mut target] {
        let mut manifest = serde_json::to_value(&profile.manifest).unwrap();
        manifest["composition"] = serde_json::json!({"schema_version":1,"routes":{"code":{"language":"inner_mold","context":"expression","scope":{"visibility":"whole_scope","parent_lookup":false,"shadowing":"nearest"}}}});
        profile.manifest = serde_json::from_value(manifest).unwrap();
        profile.validate().unwrap();
    }
    (host, target)
}

#[test]
fn declared_routes_analyze_real_interiors_with_qualified_graph_provenance() {
    for (alternate, source) in [
        (false, "VAR outside; ~[café\r\n@(f(value + 2))]~ VAR after;"),
        (true, "VAR outside; #[café\r\n@<f(value + 2)>]# VAR after;"),
    ] {
        let (host, target) = profiles(alternate);
        let analyzer = CompositionAnalyzer::new(host, vec![target]).unwrap();
        let result = analyzer.analyze_source("mixed.nxb", source).unwrap();
        assert_eq!(result.units.len(), 2);
        assert_eq!(result.interpretations.len(), 1);
        let link = &result.interpretations[0];
        let origin = result
            .units
            .iter()
            .find(|u| u.id == link.source_unit)
            .unwrap();
        let interior = result
            .units
            .iter()
            .find(|u| u.id == link.target_unit)
            .unwrap();
        assert_eq!(interior.language.0, "inner_mold");
        assert_eq!(interior.context.as_deref(), Some("expression"));
        assert_eq!(interior.status, RegionStatus::Parsed);
        let region = origin
            .graph
            .nodes
            .iter()
            .find(|n| n.id == link.region_id)
            .unwrap()
            .lexical_region
            .as_ref()
            .unwrap();
        assert_eq!(interior.span, region.content_span);
        assert_eq!(
            interior
                .tokens
                .iter()
                .map(|t| t.text.as_str())
                .collect::<String>(),
            "f(value + 2)"
        );
        let mut position = SourcePosition::default();
        position.advance(&source[..interior.span.start.offset]);
        assert_eq!(interior.span.start, position);
        assert!(interior.graph.nodes.iter().any(|n| n.expression.is_some()));
        assert!(interior.symbols.symbols.is_empty());
        for n in &interior.graph.nodes {
            if let Some(e) = &n.evidence {
                assert_eq!(e.rule_version, interior.rule_version);
                assert!(
                    e.span.start.offset >= interior.span.start.offset
                        && e.span.end.offset <= interior.span.end.offset
                );
                assert_eq!(e.path, "mixed.nxb");
            }
        }
        assert!(origin.symbols.symbols.iter().any(|s| s.name == "after"));
        assert!(!result.complete, "host template text remains opaque");
    }
}

#[test]
fn nested_discoveries_reuse_interpretations_and_keep_each_origin_link() {
    let (host, target) = profiles(false);
    let result = CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("nested.nxb", "~[outer @(f(~[inner @(value + 2)]~))]~")
        .unwrap();
    assert_eq!(result.units.len(), 3);
    assert_eq!(result.interpretations.len(), 3);
    let leaf = result
        .units
        .iter()
        .find(|u| u.tokens.iter().map(|t| t.text.as_str()).collect::<String>() == "value + 2")
        .unwrap();
    assert_eq!(
        result
            .interpretations
            .iter()
            .filter(|l| l.target_unit == leaf.id)
            .count(),
        2
    );
    for link in &result.interpretations {
        let source = result
            .units
            .iter()
            .find(|u| u.id == link.source_unit)
            .unwrap();
        assert!(source.graph.nodes.iter().any(|n| n.id == link.region_id));
        assert!(result.units.iter().any(|u| u.id == link.target_unit));
    }
}

#[test]
fn missing_dependencies_and_contexts_fail_before_source_analysis() {
    let (host, target) = profiles(false);
    assert!(CompositionAnalyzer::new(host.clone(), vec![]).is_err());
    let mut broken = target.clone();
    broken.grammar.contexts.remove("expression");
    assert!(CompositionAnalyzer::new(host.clone(), vec![broken]).is_err());
    assert!(CompositionAnalyzer::new(host, vec![target.clone(), target]).is_err());
}

#[test]
fn an_interior_failure_does_not_erase_the_host_or_neighboring_region() {
    let (host, target) = profiles(false);
    let result = CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("partial.nxb", "~[@(x @ y) @(good + 2)]~ VAR after;")
        .unwrap();
    assert_eq!(result.units.len(), 3);
    assert_eq!(
        result
            .units
            .iter()
            .filter(|u| u.status == RegionStatus::Partial)
            .count(),
        1
    );
    assert!(
        result.units[0]
            .symbols
            .symbols
            .iter()
            .any(|s| s.name == "after")
    );
    assert!(
        result
            .units
            .iter()
            .any(|u| u.status == RegionStatus::Parsed)
    );
    let failed = result
        .units
        .iter()
        .find(|u| u.status == RegionStatus::Partial)
        .unwrap();
    assert!(failed.graph.nodes.is_empty());
    assert!(!failed.diagnostics.is_empty());
    assert!(!result.complete);
}

#[test]
fn unclosed_region_and_cumulative_limits_never_report_full_success() {
    let (host, target) = profiles(false);
    let analyzer = CompositionAnalyzer::new(host.clone(), vec![target.clone()]).unwrap();
    let result = analyzer.analyze_source("cut.nxb", "~[@(value").unwrap();
    assert_eq!(result.units.len(), 2);
    assert_eq!(result.interpretations[0].status, RegionStatus::Partial);
    assert!(!result.complete);
    let limits = CompositionLimits {
        max_units: 2,
        ..Default::default()
    };
    let analyzer =
        CompositionAnalyzer::with_limits(host.clone(), vec![target.clone()], limits).unwrap();
    assert!(
        analyzer
            .analyze_source("many.nxb", "~[@(a) @(b)]~")
            .is_err()
    );
    let limits = CompositionLimits {
        max_tokens: 3,
        ..Default::default()
    };
    let analyzer = CompositionAnalyzer::with_limits(host, vec![target], limits).unwrap();
    assert!(
        analyzer
            .analyze_source("tokens.nxb", "~[@(a + b)]~")
            .is_err()
    );
}

#[test]
fn explicit_isolated_scope_resolves_locals_without_importing_or_exporting_host_names() {
    let (mut host, target) = profiles(false);
    host.manifest
        .composition
        .as_mut()
        .unwrap()
        .routes
        .get_mut("code")
        .unwrap()
        .context = "body".into();
    let result = CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source(
            "scope.nxb",
            "VAR outside; ~[@(READ outside; VAR local; READ local;)]~ READ local;",
        )
        .unwrap();
    let host = &result.units[0];
    let interior = &result.units[1];
    assert_eq!(interior.symbols.symbols.len(), 1);
    assert_eq!(interior.symbols.symbols[0].name, "local");
    assert!(!host.symbols.symbols.iter().any(|s| s.name == "local"));
    let references: Vec<_> = interior
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == nexora_graph::EdgeKind::References)
        .collect();
    assert_eq!(references.len(), 2);
    assert_eq!(
        references
            .iter()
            .filter(|e| e.resolution == nexora_graph::ResolutionState::Resolved)
            .count(),
        1
    );
    assert!(references.iter().any(|e| {
        e.resolution == nexora_graph::ResolutionState::Unresolved
            && result.source.content[e.evidence.span.start.offset..e.evidence.span.end.offset]
                .contains("outside")
    }));
}

#[test]
fn ordinary_indexer_must_not_silently_ignore_composition_routes() {
    let (host, _) = profiles(false);
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "nexora-route-requires-catalog-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("source.nxb");
    std::fs::write(&path, "~[@(x)]~").unwrap();
    let mut plain = host.clone();
    plain.manifest.composition = None;
    assert!(nexora_indexer::Indexer::new(plain).analyze(&path).is_ok());
    let result = nexora_indexer::Indexer::new(host).analyze(&path);
    std::fs::remove_dir_all(root).unwrap();
    assert!(matches!(
        result,
        Err(nexora_core::NexoraError::Configuration(_))
    ));
}

#[test]
fn profile_fingerprints_distinguish_changed_grammar_even_at_the_same_version() {
    let (host, target) = profiles(false);
    let source = "~[@(value + 2)]~";
    let a = CompositionAnalyzer::new(host.clone(), vec![target.clone()])
        .unwrap()
        .analyze_source("same.nxb", source)
        .unwrap();
    let mut edited = target;
    edited
        .grammar
        .contexts
        .get_mut("expression")
        .unwrap()
        .expression
        .as_mut()
        .unwrap()
        .infix[0]
        .precedence += 1;
    let b = CompositionAnalyzer::new(host, vec![edited])
        .unwrap()
        .analyze_source("same.nxb", source)
        .unwrap();
    assert_eq!(a.schema_version, 7);
    assert_ne!(a.configuration_fingerprint, b.configuration_fingerprint);
    assert_ne!(
        a.units[1].profile_fingerprint,
        b.units[1].profile_fingerprint
    );
    assert_ne!(a.units[1].id, b.units[1].id);
    let roundtrip: nexora_indexer::CompositionResult =
        serde_json::from_slice(&serde_json::to_vec(&a).unwrap()).unwrap();
    assert_eq!(
        a.configuration_fingerprint,
        roundtrip.configuration_fingerprint
    );
}

#[test]
fn reusable_yaml_molds_produce_linked_graphs_and_unresolved_external_reads() {
    let root = Path::new("../../templates/composition-molds");
    let host = LanguageDefinition::load(&root.join("region_carrier")).unwrap();
    let target = LanguageDefinition::load(&root.join("region_expression")).unwrap();
    let source = std::fs::read_to_string(root.join("nested.nxb")).unwrap();
    let result = CompositionAnalyzer::new(host, vec![target])
        .unwrap()
        .analyze_source("nested.nxb", &source)
        .unwrap();
    assert_eq!(result.units.len(), 4);
    assert_eq!(result.interpretations.len(), 4);
    assert!(
        result.units[1..]
            .iter()
            .all(|u| u.status != RegionStatus::Partial),
        "unresolved names do not make valid syntax partial"
    );
    assert!(result.units[1..].iter().any(|u| {
        u.graph.edges.iter().any(|e| {
            e.kind == nexora_graph::EdgeKind::References
                && e.resolution == nexora_graph::ResolutionState::Unresolved
        })
    }));
    if let Ok(path) = std::env::var("NEXORA_COMPOSITION_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    }
}

#[test]
fn catalog_fingerprint_includes_the_selected_root_profile() {
    let (host, target) = profiles(false);
    let a = CompositionAnalyzer::new(host.clone(), vec![target.clone()])
        .unwrap()
        .analyze_source("root.nxb", "VAR value;")
        .unwrap();
    let b = CompositionAnalyzer::new(target, vec![host])
        .unwrap()
        .analyze_source("root.nxb", "VAR value;")
        .unwrap();
    assert_ne!(a.source.language, b.source.language);
    assert_ne!(a.configuration_fingerprint, b.configuration_fingerprint);
}

#[test]
fn file_wide_quotas_apply_to_work_across_multiple_interpretations() {
    let (host, target) = profiles(false);
    let source = "~[@(value + 2)]~";
    for limits in [
        CompositionLimits {
            max_work_bytes: source.len() * 2,
            ..Default::default()
        },
        CompositionLimits {
            max_parser_steps: 2,
            ..Default::default()
        },
        CompositionLimits {
            max_nodes: 12,
            ..Default::default()
        },
    ] {
        let analyzer =
            CompositionAnalyzer::with_limits(host.clone(), vec![target.clone()], limits).unwrap();
        assert!(matches!(
            analyzer.analyze_source("quota.nxb", source),
            Err(nexora_core::NexoraError::Limit(_))
        ));
    }
    let mut broken = host.clone();
    broken
        .manifest
        .composition
        .as_mut()
        .unwrap()
        .routes
        .get_mut("code")
        .unwrap()
        .scope
        .parent_lookup = true;
    assert!(CompositionAnalyzer::new(broken, vec![target]).is_err());
}
