use nexora_core::TenantId;
use nexora_indexer::{IndexCache, Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::path::Path;
#[test]
fn guarded_template_view_reuses_cache_and_refreshes_parent_ranges_and_rules() {
    let directory = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "nexora-inline-template-cache-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("public.razor");
    std::fs::write(&path, "@code { var view = @<p>@Name</p>; } @(After())").unwrap();
    let profiles = Path::new("../../templates/dotnet-understanding");
    let mut host = LanguageDefinition::load(&profiles.join("razor")).unwrap();
    let child = LanguageDefinition::load(&profiles.join("csharp")).unwrap();
    // Explicitly select the pure provider to verify failed interpretations are not cached.
    let mut pure_host = host.clone();
    for route in pure_host
        .manifest
        .composition
        .as_mut()
        .unwrap()
        .routes
        .values_mut()
    {
        route.lexical_context = None;
    }
    let default = Indexer::with_catalog(pure_host, vec![child.clone()]).unwrap();
    let mut diagnostic_cache = IndexCache::default();
    let mut diagnostic_store = MemoryStore::default();
    let tenant = TenantId("inline-template-cache".into());
    let failed = default
        .index_incremental(
            &directory,
            tenant.clone(),
            &mut diagnostic_store,
            &mut diagnostic_cache,
        )
        .unwrap();
    let retried = default
        .index_incremental(
            &directory,
            tenant.clone(),
            &mut diagnostic_store,
            &mut diagnostic_cache,
        )
        .unwrap();
    assert!(
        failed
            .snapshot
            .diagnostics
            .iter()
            .any(|d| d.stage == "lexer")
    );
    assert_eq!(retried.reuse.reused_files, 0);
    assert_eq!(retried.reuse.cache_skipped_files, 1);
    assert_eq!(
        serde_json::to_value(&failed.snapshot).unwrap(),
        serde_json::to_value(&retried.snapshot).unwrap()
    );
    // This explicit structural view keeps members opaque rather than invoking the
    // pure-C# provider. The explicitly pure diagnostic view remains uncached.
    let composition = host.manifest.composition.as_mut().unwrap();
    composition.routes.remove("members");
    composition
        .roles
        .insert("members".into(), nexora_core::RegionCoverageMethod::Guard);
    let indexer = Indexer::with_catalog(host.clone(), vec![child.clone()]).unwrap();
    let mut cache = IndexCache::default();
    let mut store = MemoryStore::default();
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
    let source = "@code { var view = @<section>@Name</section>; } @(After())";
    std::fs::write(&path, source).unwrap();
    let changed = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(changed.reuse.analyzed_files, 1);
    let name = changed
        .snapshot
        .graph
        .syntax_units
        .values()
        .find(|u| {
            u.context.as_deref() == Some("expression_opaque")
                && &source[u.span.start.offset..u.span.end.offset] == "Name"
        })
        .unwrap();
    assert_eq!(name.span.start.offset, source.find("@Name").unwrap() + 1);
    let full = indexer
        .index(&directory, tenant.clone(), &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::to_value(&changed.snapshot).unwrap()
    );
    let mut altered = host;
    altered
        .lexer
        .regions
        .get_mut("members")
        .unwrap()
        .children
        .iter_mut()
        .find(|c| c.region == "body_element" && c.pattern.starts_with("@<"))
        .unwrap()
        .pattern = "@<(?P<tag>q[A-Za-z]*)".into();
    let altered = Indexer::with_catalog(altered, vec![child]).unwrap();
    let refreshed = altered
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(refreshed.reuse.analyzed_files, 1);
    assert_ne!(refreshed.snapshot.id, changed.snapshot.id);
    assert!(
        refreshed
            .snapshot
            .graph
            .nodes
            .iter()
            .filter_map(|n| n.lexical_region.as_ref())
            .all(|r| r.mode != "body_element")
    );
    let full = altered
        .index(&directory, tenant, &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::to_value(&refreshed.snapshot).unwrap()
    );
    std::fs::remove_dir_all(directory).unwrap();
}
