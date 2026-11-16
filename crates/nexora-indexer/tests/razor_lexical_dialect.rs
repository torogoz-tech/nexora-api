use nexora_core::RegionStatus;
use nexora_indexer::CompositionAnalyzer;
use nexora_language::LanguageDefinition;
use std::path::Path;

#[test]
fn declared_razor_entry_interprets_templates_without_weakening_pure_csharp() {
    let root = Path::new("../../templates/dotnet-understanding");
    let child = LanguageDefinition::load(&root.join("csharp")).unwrap();
    let lexer = nexora_lexer::Lexer::new(&child.lexer).unwrap();
    for source in ["@<p>@Name</p>", "@* local comment *@ int value = 1;"] {
        assert!(lexer.tokenize(source).is_err());
        assert!(lexer.tokenize_in_context(source, "razor").is_ok());
    }
    let analyzer = CompositionAnalyzer::new(
        LanguageDefinition::load(&root.join("razor")).unwrap(),
        vec![child],
    )
    .unwrap();
    let source = "@code { @* } hidden *@ var view = @<p>; class Ghost { } @Name</p>; int after=1; } @(After())";
    let result = analyzer.analyze_source("public.razor", source).unwrap();
    assert!(
        result.units.iter().all(|u| u.diagnostics.is_empty()),
        "{:?}",
        result
            .units
            .iter()
            .flat_map(|u| &u.diagnostics)
            .collect::<Vec<_>>()
    );
    assert_eq!(result.units.len(), 4);
    let member = result
        .units
        .iter()
        .find(|u| u.context.as_deref() == Some("members_opaque"))
        .unwrap();
    assert_eq!(member.status, RegionStatus::Opaque);
    assert_eq!(member.lexical_context.as_deref(), Some("razor"));
    for name in ["Name", "After()"] {
        let units: Vec<_> = result
            .units
            .iter()
            .filter(|u| &source[u.span.start.offset..u.span.end.offset] == name)
            .collect();
        assert_eq!(units.len(), 1, "duplicate or missing {name}");
        assert_eq!(units[0].lexical_context.as_deref(), Some("razor"));
    }
    assert!(
        result
            .graph
            .edges
            .iter()
            .all(|e| e.kind != nexora_graph::EdgeKind::Calls)
    );
}

#[test]
fn mixed_interpolation_protects_strings_and_recurses_into_template_expressions() {
    let root = Path::new("../../templates/dotnet-understanding");
    let analyzer = CompositionAnalyzer::new(
        LanguageDefinition::load(&root.join("razor")).unwrap(),
        vec![LanguageDefinition::load(&root.join("csharp")).unwrap()],
    )
    .unwrap();
    let source = r#"@code { var literal="@<p>@(Fake())</p>"; var value=$"{Render(@<p>@(Real())</p>)}"; } @(After())"#;
    let result = analyzer.analyze_source("public.razor", source).unwrap();
    assert!(
        result.units.iter().all(|u| u.diagnostics.is_empty()),
        "{:?}",
        result
            .units
            .iter()
            .flat_map(|u| &u.diagnostics)
            .collect::<Vec<_>>()
    );
    for name in ["Real()", "After()"] {
        assert_eq!(
            result
                .units
                .iter()
                .filter(|u| &source[u.span.start.offset..u.span.end.offset] == name)
                .count(),
            1
        );
    }
    assert!(
        result
            .units
            .iter()
            .all(|u| &source[u.span.start.offset..u.span.end.offset] != "Fake()")
    );
    assert!(
        result
            .units
            .iter()
            .all(|u| u.status != RegionStatus::Partial)
    );
    let member = result
        .units
        .iter()
        .find(|u| u.context.as_deref() == Some("members_opaque"))
        .unwrap();
    assert!(
        member
            .tokens
            .iter()
            .flat_map(|t| &t.regions)
            .any(|r| r.mode == "c_interpolated_regular")
    );
    assert!(
        result
            .units
            .iter()
            .skip(1)
            .all(|u| u.lexical_context.as_deref() == Some("razor"))
    );
}

#[test]
fn declared_dialect_cache_reuses_units_and_invalidates_provider_rules() {
    use nexora_core::TenantId;
    use nexora_indexer::{IndexCache, Indexer, MemoryStore};
    let directory = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-declared-dialect-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = "@code { var view=@<p>@Name</p>; } @(After())";
    std::fs::write(directory.join("public.razor"), source).unwrap();
    let root = Path::new("../../templates/dotnet-understanding");
    let host = LanguageDefinition::load(&root.join("razor")).unwrap();
    let mut child = LanguageDefinition::load(&root.join("csharp")).unwrap();
    let indexer = Indexer::with_catalog(host.clone(), vec![child.clone()]).unwrap();
    let tenant = TenantId("public-dialect".into());
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
    let cold = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    let warm = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert!(cold.snapshot.diagnostics.is_empty());
    assert_eq!(warm.reuse.reused_files, 1);
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    child
        .lexer
        .entry_contexts
        .get_mut("razor")
        .unwrap()
        .retain(|kind| kind != "razor_template_element" && kind != "razor_template_unknown");
    let changed = Indexer::with_catalog(host, vec![child]).unwrap();
    let refreshed = changed
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(refreshed.reuse.analyzed_files, 1);
    assert_ne!(refreshed.snapshot.id, cold.snapshot.id);
    assert!(
        refreshed
            .snapshot
            .diagnostics
            .iter()
            .any(|d| d.stage == "lexer")
    );
    let full = changed
        .index(&directory, tenant, &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::to_value(&refreshed.snapshot).unwrap()
    );
    std::fs::remove_dir_all(directory).unwrap();
}
