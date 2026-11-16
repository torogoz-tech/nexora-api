use nexora_core::TenantId;
use nexora_indexer::{IndexCache, Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::path::Path;
#[test]
fn trivia_region_rule_edits_invalidate_chain_context_and_match_full_analysis() {
    let directory = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-trivia-region-cache-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("public.razor"),
        "@if (first) { <p>@A</p> } @* note *@ else if (second) { <p>@B</p> } else { <p>@C</p> }",
    )
    .unwrap();
    let profiles = Path::new("../../templates/dotnet-understanding");
    let host = LanguageDefinition::load(&profiles.join("razor")).unwrap();
    let child = LanguageDefinition::load(&profiles.join("csharp")).unwrap();
    let indexer = Indexer::with_catalog(host.clone(), vec![child.clone()]).unwrap();
    let tenant = TenantId("public-trivia-cache".into());
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
    assert_eq!(
        cold.snapshot
            .graph
            .nodes
            .iter()
            .filter_map(|n| n.lexical_region.as_ref())
            .filter(|r| r.mode == "conditional")
            .count(),
        2
    );
    let mut changed = host;
    changed
        .lexer
        .tokens
        .iter_mut()
        .find(|t| t.kind == "razor_comment")
        .unwrap()
        .trivia = false;
    let changed = Indexer::with_catalog(changed, vec![child]).unwrap();
    let refreshed = changed
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(refreshed.reuse.analyzed_files, 1);
    assert_ne!(refreshed.snapshot.id, cold.snapshot.id);
    assert_eq!(
        refreshed
            .snapshot
            .graph
            .nodes
            .iter()
            .filter_map(|n| n.lexical_region.as_ref())
            .filter(|r| r.mode == "conditional")
            .count(),
        1
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
