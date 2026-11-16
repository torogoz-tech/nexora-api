#[path = "../../nexora-platform-store/tests/support/local_snapshot_fixture.rs"]
mod local_snapshot_fixture;
#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use local_snapshot_fixture::LocalSnapshotFixture;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::{Capability, KeyScope, LocalSourceChange};
use serde_json::{Value, json};
use tower::ServiceExt;

#[test]
#[ignore = "requires local databases; REST/MCP share seven code reads and current scoped source authority"]
fn rest_code_reads_match_mcp_and_never_accept_client_authority() {
    let mut f = LocalSnapshotFixture::new();
    let published = f.adopt();
    let project = f.work.project.id.clone();
    let repository = published.repository_id.clone();
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Code REST fixture",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead, Capability::KnowledgeRead],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let executor = tokio::runtime::Runtime::new().unwrap();
    let app = executor.block_on(async {
        ApiServer::router(
            ApiState::new(f.work.config.clone(), f.work.namespace.clone(), 2)
                .await
                .unwrap(),
        )
    });
    let http = |path: &str, token: Option<&str>, body: Value| {
        executor.block_on(async {
            let mut request = Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .header("mcp-protocol-version", "2025-11-25");
            if let Some(token) = token {
                request = request.header("authorization", format!("Bearer {token}"));
            }
            let response = app
                .clone()
                .oneshot(
                    request
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (
                status,
                serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            )
        })
    };
    let route = format!("/api/v1/projects/{project}/repositories/{repository}/code/query");
    let receipts_before: i64 = f
        .sql
        .query_one("SELECT count(*) FROM request_receipts", &[])
        .unwrap()
        .get(0);
    let first = json!({"operation":"overview","arguments":{"limit":5}});
    let (status, overview) = http(&route, Some(&key.key), first.clone());
    assert_eq!(
        status,
        StatusCode::OK,
        "registered REST code-query route must exist"
    );
    assert_eq!(overview["snapshot"], f.header.id.0);
    let snapshot = f
        .runtime
        .snapshot(
            &f.header.tenant_id,
            &f.header.repository_id,
            Some(&f.header.id),
        )
        .unwrap()
        .unwrap();
    let node = snapshot
        .graph
        .nodes
        .iter()
        .find(|node| node.name == "retained_identity")
        .unwrap()
        .id
        .0
        .clone();
    let cases = [
        ("overview", "nexora_repository_overview", json!({"limit":5})),
        (
            "find_symbol",
            "nexora_find_symbol",
            json!({"name":"retained_identity","limit":5}),
        ),
        (
            "references",
            "nexora_find_references",
            json!({"node":node,"limit":5}),
        ),
        (
            "dependencies",
            "nexora_get_dependencies",
            json!({"node":node,"limit":5}),
        ),
        (
            "traverse",
            "nexora_traverse_graph",
            json!({"node":node,"depth":1,"limit":10}),
        ),
        (
            "search",
            "nexora_search_code",
            json!({"query":"retained_identity","limit":5}),
        ),
        (
            "context",
            "nexora_get_source_context",
            json!({"path":"source.js","start_line":1,"line_count":1}),
        ),
    ];
    for (operation, tool, args) in cases {
        let (status, value) = http(
            &route,
            Some(&key.key),
            json!({"operation":operation,"arguments":args}),
        );
        assert_eq!(status, StatusCode::OK, "{operation}");
        let mut scoped = args.as_object().unwrap().clone();
        scoped.insert("project_id".into(), json!(project));
        scoped.insert("repository_id".into(), json!(repository));
        let (status, rpc) = http(
            "/mcp",
            Some(&key.key),
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":tool,"arguments":scoped}}),
        );
        assert_eq!(status, StatusCode::OK);
        assert_eq!(rpc["result"]["isError"], false);
        assert_eq!(
            value, rpc["result"]["structuredContent"],
            "REST and MCP diverged for {operation}"
        );
    }
    assert_eq!(
        http(&route, None, first.clone()).0,
        StatusCode::UNAUTHORIZED
    );
    for field in ["actor", "tenant", "root", "project_id", "repository_id"] {
        let mut args = json!({"limit":5});
        args[field] = json!("forged");
        assert_eq!(
            http(
                &route,
                Some(&key.key),
                json!({"operation":"overview","arguments":args})
            )
            .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    assert_eq!(
        http(
            &route,
            Some(&key.key),
            json!({"operation":"overview","actor":"forged"})
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        http(
            &route,
            Some(&key.key),
            json!({"operation":"overview","arguments":{"limit":0}})
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        http(&route, Some(&key.key), json!({"operation":"unknown"})).0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        http(
            &route,
            Some(&key.key),
            json!({"operation":"overview","arguments":{"snapshot":"not-attested"}})
        )
        .0,
        StatusCode::NOT_FOUND
    );
    let foreign = route.replace(&project, "prj_other");
    assert_eq!(
        http(&foreign, Some(&key.key), first.clone()).0,
        StatusCode::NOT_FOUND
    );
    let receipts_after: i64 = f
        .sql
        .query_one("SELECT count(*) FROM request_receipts", &[])
        .unwrap()
        .get(0);
    assert_eq!(
        receipts_before, receipts_after,
        "read-only POST must not create mutation receipts"
    );
    let change = LocalSourceChange {
        expected_revision: published.source_revision,
        reason: "Revoke fixture after warm reads".into(),
    };
    let operator = f.work.ctx.clone();
    f.work
        .store
        .request(
            &operator,
            "local_source.revoke",
            "rest-code-revoke",
            &json!({"project_id":project,"repository_id":repository,"change":change}),
            |store| store.revoke_local_source(&operator, &project, &repository, &change),
        )
        .unwrap();
    assert_eq!(
        http(&route, Some(&key.key), first.clone()).0,
        StatusCode::NOT_FOUND
    );
    f.work
        .store
        .revoke_key(&f.work.ctx, &key.metadata.id)
        .unwrap();
    assert_eq!(
        http(&route, Some(&key.key), first).0,
        StatusCode::UNAUTHORIZED
    );
}
