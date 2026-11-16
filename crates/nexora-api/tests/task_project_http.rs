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
#[ignore = "requires PostgreSQL; explicit REST/MCP project transfer with live permissions and replay"]
fn task_project_change_is_shared_by_rest_and_mcp() {
    let mut f = WorkItemFixture::new();
    let task=f.store.create_pool_task(&f.ctx,&serde_json::from_value(json!({"project_id":null,"title":"Move this task","type_code":"task","origin":"fixture","fields":{}})).unwrap()).unwrap();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Project change fixture",
            KeyScope {
                capabilities: vec![Capability::TasksRead, Capability::TasksWrite],
                project_ids: vec![],
                all_projects: true,
                installation: true,
            },
            3600,
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
    let call = |path: &str, body: Value, receipt: &str| {
        runtime.block_on(async {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(path)
                        .header("authorization", format!("Bearer {}", key.key))
                        .header("accept", "application/json, text/event-stream")
                        .header("content-type", "application/json")
                        .header("mcp-protocol-version", "2025-11-25")
                        .header("idempotency-key", receipt)
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
    let path = format!("/api/v1/tasks/{}/project", task.id);
    assert_eq!(
        call(
            &path,
            json!({"expected_revision":task.revision,"reason":"Missing destination"}),
            "missing-project"
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let input = json!({"project_id":f.project.id,"expected_revision":task.revision,"reason":"Assign project"});
    let moved = call(&path, input.clone(), "move-project");
    assert_eq!(moved.0, StatusCode::OK, "{}", moved.1);
    assert_eq!(moved.1["project_id"], f.project.id);
    assert_eq!(call(&path, input, "move-project"), moved);
    let mcp = call(
        "/mcp",
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nexora_task_project_change","arguments":{"task_id":task.id,"request_id":"return-team-pool","change":{"project_id":null,"expected_revision":moved.1["revision"],"reason":"Team task"}}}}),
        "mcp-project",
    );
    assert_eq!(mcp.0, StatusCode::OK);
    assert_eq!(mcp.1["result"]["isError"], false, "{}", mcp.1);
    assert!(mcp.1["result"]["structuredContent"]["project_id"].is_null());
    assert_eq!(mcp.1["result"]["structuredContent"]["id"], task.id);
}
