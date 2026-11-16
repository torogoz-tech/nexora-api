use nexora_core::TenantId;
use nexora_indexer::{IndexCache, Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use std::{fs, path::Path};

#[test]
fn body_expression_context_reuse_and_same_version_edit_match_full_analysis() {
    let directory = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "nexora-body-expression-cache-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("public.razor"),
        "@if(Ready){@Value} @(After())",
    )
    .unwrap();
    let root = Path::new("../../templates/dotnet-understanding");
    let host = LanguageDefinition::load(&root.join("razor")).unwrap();
    let provider = LanguageDefinition::load(&root.join("csharp")).unwrap();
    let indexer = Indexer::with_catalog(host.clone(), vec![provider.clone()]).unwrap();
    let tenant = TenantId("public-body-expression".into());
    let mut store = MemoryStore::default();
    let mut cache = IndexCache::default();
    let cold = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert!(cold.snapshot.diagnostics.is_empty());
    let warm = indexer
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(warm.reuse.reused_files, 1);
    assert_eq!(
        serde_json::to_value(&cold.snapshot).unwrap(),
        serde_json::to_value(&warm.snapshot).unwrap()
    );
    let mut changed = host;
    changed
        .lexer
        .regions
        .get_mut("conditional")
        .unwrap()
        .token_rules
        .retain(|category| category != "implicit_expression");
    let changed = Indexer::with_catalog(changed, vec![provider]).unwrap();
    let refreshed = changed
        .index_incremental(&directory, tenant.clone(), &mut store, &mut cache)
        .unwrap();
    assert_eq!(refreshed.reuse.analyzed_files, 1);
    assert_ne!(warm.snapshot.id, refreshed.snapshot.id);
    assert!(!refreshed.snapshot.diagnostics.is_empty());
    let full = changed
        .index(&directory, tenant, &mut MemoryStore::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(full).unwrap(),
        serde_json::to_value(refreshed.snapshot).unwrap()
    );
    fs::remove_dir_all(directory).unwrap();
}
