#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::{Value, json};
use tower::ServiceExt;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL"]
fn request_mcp_catalog_and_calls_are_isolated_from_it() {
    let mut f = WorkItemFixture::new();
    let password = nexora_platform_store::SecretGenerator::token("fixture").unwrap();
    let actor = f
        .store
        .create_user(&f.ctx, "request_mcp", &password, "Requester")
        .unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &actor.id,
            &MemberRoleChange {
                role: MemberRole::User,
                all_projects: false,
                project_ids: vec![],
                expected_revision: 0,
            },
        )
        .unwrap();
    let session = f.store.login("request_mcp", &password).unwrap();
    let gate = f
        .store
        .identity_gate(&session.access_token, "skip", "")
        .unwrap();
    let ctx = f
        .store
        .authenticate(gate["access_token"].as_str().unwrap())
        .unwrap();
    let key = f
        .store
        .issue_personal_mcp_key(
            &ctx,
            &PersonalMcpKeyCreate {
                name: "Request client".into(),
                project_ids: vec![],
                read_only: false,
                include_global: true,
                expires_in_seconds: 86400,
            },
        )
        .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let app = runtime.block_on(async {
        ApiServer::router(
            ApiState::new(f.config.clone(), f.namespace.clone(), 4)
                .await
                .unwrap(),
        )
    });
    let call = |path: &str, body: Value| {
        runtime.block_on(async {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(path)
                        .header("authorization", format!("Bearer {}", key.key))
                        .header("content-type", "application/json")
                        .header("accept", "application/json, text/event-stream")
                        .header("mcp-protocol-version", "2025-11-25")
                        .body(Body::from(body.to_string()))
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
    let catalog = call(
        "/mcp",
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
    );
    assert_eq!(catalog.0, StatusCode::OK);
    let tools = catalog.1["result"]["tools"]
        .as_array()
        .expect("MCP request catalog");
    assert_eq!(tools.len(), 10);
    assert!(tools.iter().all(|t| t["name"] == "nexora_permissions_get"
        || t["name"].as_str().unwrap().starts_with("nexora_request_")));
    assert!(!tools.iter().any(|t| t["name"] == "nexora_request_change"));
    let created = call(
        "/mcp",
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nexora_request_create","arguments":{"request_id":"mcp-create-1","title":"Cannot print","body":"## Evidence\nPrinter offline","type_id":"support"}}}),
    );
    assert_eq!(created.1["result"]["isError"], false, "{created:?}");
    let report = call(
        "/mcp",
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nexora_request_report","arguments":{}}}),
    );
    assert_eq!(report.1["result"]["isError"], false, "{report:?}");
    let response = runtime
        .block_on(
            app.clone().oneshot(
                Request::builder()
                    .uri("/api/v1/requests/reports?format=csv")
                    .header(
                        "authorization",
                        format!("Bearer {}", gate["access_token"].as_str().unwrap()),
                    )
                    .body(Body::empty())
                    .unwrap(),
            ),
        )
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/csv")
    );
    let forbidden = call(
        "/mcp",
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nexora_projects_list","arguments":{}}}),
    );
    assert_eq!(forbidden.1["result"]["isError"], true);
    assert_eq!(call("/api/v1/tasks", json!({})).0, StatusCode::FORBIDDEN);
    assert_eq!(
        f.store.service_requests(&ctx, "", "", 1).unwrap()["total"],
        1
    );
}
