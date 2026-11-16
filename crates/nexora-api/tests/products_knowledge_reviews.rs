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
fn mcp_requests_workflow_free_review_and_http_enforces_human_decisions() {
    let mut f = WorkItemFixture::new();
    let key = f
        .store
        .issue_personal_mcp_key(
            &f.ctx,
            &PersonalMcpKeyCreate {
                name: "Review MCP".into(),
                project_ids: vec![],
                read_only: false,
                include_global: true,
                expires_in_seconds: 86400,
            },
        )
        .unwrap();
    let doc = f
        .store
        .organization_knowledge_write(
            &f.ctx,
            None,
            &OrganizationDocumentWrite {
                kind: "requirement".into(),
                title: "OTP".into(),
                body: "Confirm identity".into(),
                expected_revision: None,
            },
        )
        .unwrap();
    let executor = tokio::runtime::Runtime::new().unwrap();
    let app = executor.block_on(async {
        ApiServer::router(
            ApiState::new(f.config.clone(), f.namespace.clone(), 2)
                .await
                .unwrap(),
        )
    });
    let send = |path: &str, body: Value| {
        executor.block_on(async {
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
                        .header("idempotency-key", "fixture-review-decision")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (status, serde_json::from_slice::<Value>(&bytes).unwrap())
        })
    };
    let rpc = |name: &str, args: Value| {
        send(
            "/mcp",
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}),
        )
    };
    let (status, response) = rpc(
        "nexora_knowledge_review_request",
        json!({"request_id":"review-otp","kind":"organization_document","resource_id":doc["id"],"expected_revision":1}),
    );
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["result"]["isError"], false, "{response}");
    let id = response["result"]["structuredContent"]["id"]
        .as_str()
        .unwrap();
    let (_, page) = rpc("nexora_knowledge_review_list", json!({}));
    assert_eq!(page["result"]["structuredContent"]["total"], 1, "{page}");
    let (status, _) = send(
        &format!("/api/v1/knowledge-reviews/{id}"),
        json!({"decision":"approved","expected_revision":1}),
    );
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, products) = rpc("nexora_request_products_list", json!({}));
    assert_eq!(products["result"]["isError"], false, "{products}");
}
