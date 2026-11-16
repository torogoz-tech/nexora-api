use nexora_core::TenantId;
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use nexora_mcp::SnapshotQuery;
use nexora_runtime::{DatabaseConnections, DatabaseRuntime, KnowledgeQuery};
use serde_json::json;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
#[ignore = "requires real databases; verifies selective query result and evidence parity"]
fn indexed_reads_match_canonical_query_results_for_composed_units() {
    let root = Path::new("../../templates/composition-calls");
    let host = LanguageDefinition::load(&root.join("region_carrier")).unwrap();
    let target = LanguageDefinition::load(&root.join("region_expression")).unwrap();
    let indexer = Indexer::with_catalog(host, vec![target]).unwrap();
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tenant = TenantId(format!("public-query-parity-{unique}"));
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let mut runtime = DatabaseRuntime::connect(&config).unwrap();
    let result = runtime
        .index(&indexer, &root.join("fixtures/complete"), tenant.clone())
        .unwrap();
    let s = &result.snapshot;
    for offset in [0, 1, 1000] {
        for name in ["run", "missing"] {
            let actual = runtime
                .query(
                    &tenant,
                    &s.repository_id,
                    Some(&s.id),
                    &KnowledgeQuery::FindSymbol {
                        name: name.into(),
                        offset,
                        limit: 5,
                    },
                )
                .unwrap();
            assert_eq!(
                actual,
                SnapshotQuery::run(
                    s,
                    "nexora_find_symbol",
                    &json!({"name":name,"offset":offset,"limit":5})
                )
                .unwrap()
            );
        }
        assert_eq!(
            runtime
                .query(
                    &tenant,
                    &s.repository_id,
                    Some(&s.id),
                    &KnowledgeQuery::Overview { offset, limit: 5 }
                )
                .unwrap(),
            SnapshotQuery::run(
                s,
                "nexora_repository_overview",
                &json!({"offset":offset,"limit":5})
            )
            .unwrap()
        );
    }
    for node in &s.graph.nodes {
        for (name, query) in [
            (
                "nexora_find_references",
                KnowledgeQuery::References {
                    node: node.id.clone(),
                    offset: 0,
                    limit: 100,
                },
            ),
            (
                "nexora_get_dependencies",
                KnowledgeQuery::Dependencies {
                    node: node.id.clone(),
                    offset: 0,
                    limit: 100,
                },
            ),
        ] {
            assert_eq!(
                runtime
                    .query(&tenant, &s.repository_id, Some(&s.id), &query)
                    .unwrap(),
                SnapshotQuery::run(s, name, &json!({"node":node.id.0,"limit":100})).unwrap()
            );
        }
    }
    for term in ["FUNC", "run", "missing", "<", ""] {
        let query = KnowledgeQuery::Search {
            query: term.into(),
            offset: 0,
            limit: 10,
        };
        if term.is_empty() {
            assert!(
                runtime
                    .query(&tenant, &s.repository_id, Some(&s.id), &query)
                    .is_err()
            );
            continue;
        }
        assert_eq!(
            runtime
                .query(&tenant, &s.repository_id, Some(&s.id), &query)
                .unwrap(),
            SnapshotQuery::run(s, "nexora_search_code", &json!({"query":term,"limit":10})).unwrap()
        );
    }
    for path in s.sources.keys() {
        for start_line in [1, 2, 1000] {
            assert_eq!(
                runtime
                    .query(
                        &tenant,
                        &s.repository_id,
                        Some(&s.id),
                        &KnowledgeQuery::Context {
                            path: path.clone(),
                            start_line,
                            line_count: 5
                        }
                    )
                    .unwrap(),
                SnapshotQuery::run(
                    s,
                    "nexora_get_source_context",
                    &json!({"path":path,"start_line":start_line,"line_count":5})
                )
                .unwrap()
            );
        }
    }
}
