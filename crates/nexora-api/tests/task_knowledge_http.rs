#[path = "../../nexora-platform-store/tests/support/feature_map_fixture.rs"]
mod feature_map_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use feature_map_fixture::FeatureMapFixture;
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::{Value, json};
use tower::ServiceExt;
#[test]
#[ignore = "requires PostgreSQL; web and MCP knowledge relationships with governed features"]
fn task_knowledge_links_replay_and_respect_authority() {
    let mut f = FeatureMapFixture::new();
    let proposal_input = f.proposal("Authentication", false);
    let proposal = f
        .work
        .store
        .propose_feature_map(&f.developer, &f.work.project.id, &proposal_input)
        .unwrap();
    let approved = f
        .work
        .store
        .decide_feature_map(
            &f.pmo,
            &f.work.project.id,
            proposal["id"].as_str().unwrap(),
            &FeatureMapFixture::decision(true),
        )
        .unwrap();
    let feature = approved["result"]["feature_ids"][0]
        .as_str()
        .unwrap()
        .to_string();
    let task = f.work.task("Link existing knowledge");
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Knowledge fixture",
            KeyScope {
                capabilities: vec![
                    Capability::TasksRead,
                    Capability::TasksWrite,
                    Capability::DocumentsRead,
                    Capability::DocumentsWrite,
                ],
                project_ids: vec![],
                all_projects: true,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let narrow = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Task only fixture",
            KeyScope {
                capabilities: vec![Capability::TasksRead, Capability::TasksWrite],
                project_ids: vec![],
                all_projects: true,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let write_only = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Write-only fixture",
            KeyScope {
                capabilities: vec![Capability::TasksWrite, Capability::DocumentsRead],
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
            ApiState::new(f.work.config.clone(), f.work.namespace.clone(), 4)
                .await
                .unwrap(),
        )
    });
    let call = |method: &str, path: &str, token: &str, value: Option<Value>, receipt: &str| {
        runtime.block_on(async {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header("authorization", format!("Bearer {token}"))
                        .header("accept", "application/json, text/event-stream")
                        .header("content-type", "application/json")
                        .header("mcp-protocol-version", "2025-11-25")
                        .header("idempotency-key", receipt)
                        .body(value.map_or_else(Body::empty, |v| Body::from(v.to_string())))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let body = response.into_body().collect().await.unwrap().to_bytes();
            (
                status,
                serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null),
            )
        })
    };
    let path = format!("/api/v1/tasks/{}/knowledge", task.id);
    let link = json!({"kind":"feature","target_id":feature,"expected_revision":task.revision,"expected_target_revision":1,"reason":"Existing capability"});
    assert_eq!(
        call(
            "POST",
            &path,
            &write_only.key,
            Some(link.clone()),
            "denied-unreadable-task"
        )
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            "POST",
            &path,
            &narrow.key,
            Some(link.clone()),
            "denied-feature"
        )
        .0,
        StatusCode::FORBIDDEN
    );
    let linked = call("POST", &path, &key.key, Some(link.clone()), "link-feature");
    assert_eq!(linked.0, StatusCode::OK, "{}", linked.1);
    assert_eq!(
        call("POST", &path, &key.key, Some(link), "link-feature"),
        linked
    );
    let requirement = json!({"kind":"requirement","target_id":f.requirement.id,"version_id":f.requirement.latest_version_id,"expected_revision":linked.1["revision"],"expected_target_revision":f.requirement.revision,"reason":"Reference exact requirement"});
    let linked_requirement = call(
        "POST",
        &path,
        &key.key,
        Some(requirement.clone()),
        "link-requirement",
    );
    assert_eq!(
        linked_requirement.0,
        StatusCode::OK,
        "{}",
        linked_requirement.1
    );
    assert_eq!(
        call(
            "POST",
            &path,
            &key.key,
            Some(requirement),
            "link-requirement"
        ),
        linked_requirement
    );
    let page = f
        .work
        .store
        .pool_task_presentation(&f.work.ctx, &task.id)
        .unwrap();
    assert_eq!(page["knowledge"]["features"][0]["id"], feature);
    assert_eq!(page["knowledge"]["documents"][0]["id"], f.requirement.id);
    let get = call(
        "POST",
        "/mcp",
        &key.key,
        Some(
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nexora_task_get","arguments":{"project_id":f.work.project.id,"task_id":task.id}}}),
        ),
        "mcp-get",
    );
    assert_eq!(get.0, StatusCode::OK);
    assert_eq!(
        get.1["result"]["structuredContent"]["knowledge"]["features"][0]["id"],
        feature
    );
    let arguments = json!({"task_id":task.id,"request_id":"mcp-link-existing-feature","link":{"kind":"feature","target_id":feature,"expected_revision":page["revision"],"expected_target_revision":1,"reason":"MCP existing capability"}});
    let mcp = call(
        "POST",
        "/mcp",
        &key.key,
        Some(
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nexora_task_knowledge_link","arguments":arguments}}),
        ),
        "mcp-link",
    );
    assert_eq!(mcp.1["result"]["isError"], false, "{}", mcp.1);
    let tools = call(
        "POST",
        "/mcp",
        &key.key,
        Some(json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}})),
        "mcp-tools",
    );
    assert!(
        tools.1["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "nexora_task_knowledge_link")
    );
}
