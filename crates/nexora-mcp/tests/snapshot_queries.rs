use nexora_core::TenantId;
use nexora_indexer::{Indexer, MemoryStore};
use nexora_language::LanguageDefinition;
use nexora_mcp::{SnapshotQuery, ToolCatalog};
use serde_json::json;

fn snapshot() -> nexora_storage::Snapshot {
    let language =
        LanguageDefinition::load(std::path::Path::new("../../languages/javascript")).unwrap();
    Indexer::new(language)
        .index(
            std::path::Path::new("../../tests/fixtures/javascript"),
            TenantId("mcp-public".into()),
            &mut MemoryStore::default(),
        )
        .unwrap()
}

#[test]
fn catalog_and_strict_arguments_reflect_actual_access_and_bounds() {
    let read = ToolCatalog::list(false);
    assert_eq!(read.len(), 10);
    assert!(!read.iter().any(|t| t["name"] == "nexora_index_repository"));
    assert_eq!(ToolCatalog::list(true).len(), 11);
    assert!(
        ToolCatalog::validate(
            "nexora_find_symbol",
            &json!({"name":"hello","limit":5}),
            false
        )
        .is_ok()
    );
    for args in [
        json!({"name":""}),
        json!({"name":"hello","tenant":"other"}),
        json!({"name":"hello","limit":0}),
        json!({"name":"hello","limit":101}),
        json!({"name":"hello","limit":1.5}),
        json!({"name":4}),
        json!([]),
    ] {
        assert!(
            ToolCatalog::validate("nexora_find_symbol", &args, false).is_err(),
            "{args}"
        );
    }
    assert!(ToolCatalog::validate("nexora_index_repository", &json!({}), false).is_err());
    assert!(
        ToolCatalog::validate(
            "nexora_get_source_context",
            &json!({"path":"../secret","start_line":0}),
            false
        )
        .is_err()
    );
}

#[test]
fn queries_preserve_snapshot_identity_evidence_and_exact_pagination() {
    let s = snapshot();
    let overview = SnapshotQuery::run(&s, "nexora_repository_overview", &json!({})).unwrap();
    assert_eq!(overview["snapshot"], s.id.0);
    assert_eq!(overview["repository"], s.repository_id.0);
    assert_eq!(overview["tenant"], s.tenant_id.0);
    assert_eq!(overview["nodes"], s.graph.nodes.len());
    let named = s.graph.nodes.iter().find(|n| n.evidence.is_some()).unwrap();
    let result = SnapshotQuery::run(
        &s,
        "nexora_find_symbol",
        &json!({"name":named.name,"limit":1}),
    )
    .unwrap();
    assert_eq!(result["items"][0], serde_json::to_value(named).unwrap());
    let page = SnapshotQuery::run(
        &s,
        "nexora_find_symbol",
        &json!({"name":named.name,"offset":result["total"].as_u64().unwrap()}),
    )
    .unwrap();
    assert_eq!(page["items"], json!([]));
    assert_eq!(page["truncated"], false);
    assert!(
        SnapshotQuery::run(
            &s,
            "nexora_find_symbol",
            &json!({"name":named.name,"tenant":"other"})
        )
        .is_err()
    );
}

#[test]
fn source_context_uses_snapshot_only_and_unicode_snippets_are_bounded() {
    let mut s = snapshot();
    s.sources.insert(
        "unicode.txt".into(),
        format!("first\n{}\nlast", "á".repeat(40_000)),
    );
    let context = SnapshotQuery::run(
        &s,
        "nexora_get_source_context",
        &json!({"path":"unicode.txt","start_line":2,"line_count":1}),
    )
    .unwrap();
    assert_eq!(context["items"][0]["line"], 2);
    assert_eq!(context["items"][0]["text_truncated"], true);
    assert!(context["items"][0]["text"].as_str().unwrap().len() <= 8192);
    assert_eq!(context["next_line"], 3);
    let search = SnapshotQuery::run(&s, "nexora_search_code", &json!({"query":"last"})).unwrap();
    assert_eq!(search["items"][0]["path"], "unicode.txt");
    assert_eq!(search["items"][0]["line"], 3);
    assert!(
        SnapshotQuery::run(
            &s,
            "nexora_get_source_context",
            &json!({"path":"../../Cargo.toml"})
        )
        .is_err()
    );
    assert!(SnapshotQuery::run(&s, "nexora_search_code", &json!({"query":""})).is_err());
}

#[test]
fn semantic_edges_are_returned_only_with_their_actual_resolution_and_evidence() {
    use nexora_graph::{EdgeKind, ResolutionState};
    use std::path::Path;
    for (case, state) in [
        ("fail", ResolutionState::Resolved),
        ("external", ResolutionState::Unresolved),
        ("missing", ResolutionState::Unresolved),
        ("ambiguous", ResolutionState::Ambiguous),
    ] {
        let language = LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap();
        let s = Indexer::new(language)
            .index(
                &Path::new("../../policies/architecture/domain-boundary/fixtures").join(case),
                TenantId("mcp-edges".into()),
                &mut MemoryStore::default(),
            )
            .unwrap();
        let dependency = s
            .graph
            .edges
            .iter()
            .find(|e| e.kind == EdgeKind::Imports)
            .unwrap();
        let deps = SnapshotQuery::run(
            &s,
            "nexora_get_dependencies",
            &json!({"node":dependency.source.0}),
        )
        .unwrap();
        assert_eq!(deps["items"].as_array().unwrap().len(), 2);
        assert!(
            deps["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e["resolution"] == json!(state))
        );
        let refs = SnapshotQuery::run(
            &s,
            "nexora_find_references",
            &json!({"node":dependency.target.0}),
        )
        .unwrap();
        assert!(
            refs["items"]
                .as_array()
                .unwrap()
                .contains(&serde_json::to_value(dependency).unwrap())
        );
        assert!(
            refs["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e["kind"] != "CONTAINS" && e["kind"] != "DECLARES")
        );
        assert!(
            SnapshotQuery::run(
                &s,
                "nexora_get_dependencies",
                &json!({"node":"missing-node"})
            )
            .is_err()
        );
    }
    let root = Path::new("../../templates/composition-calls");
    let host = LanguageDefinition::load(&root.join("region_carrier")).unwrap();
    let target = LanguageDefinition::load(&root.join("region_expression")).unwrap();
    let s = Indexer::with_catalog(host, vec![target])
        .unwrap()
        .index(
            &root.join("fixtures/complete"),
            TenantId("mcp-calls".into()),
            &mut MemoryStore::default(),
        )
        .unwrap();
    let calls: Vec<_> = s
        .graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls)
        .collect();
    assert_eq!(calls.len(), 2);
    for call in calls {
        let refs = SnapshotQuery::run(&s, "nexora_find_references", &json!({"node":call.target.0}))
            .unwrap();
        assert!(
            refs["items"]
                .as_array()
                .unwrap()
                .contains(&serde_json::to_value(call).unwrap())
        );
        assert!(call.call_binding.is_some());
    }
}
