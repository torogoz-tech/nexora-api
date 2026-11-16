use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::{Capability, KeyScope};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
use zeroize::Zeroizing;

#[test]
#[ignore = "requires PostgreSQL; task movement and explicit workflow migration over HTTP/MCP"]
fn task_movement_and_explicit_policy_migration_share_http_mcp_receipts() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_task_lifecycle_http_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    store
        .bootstrap("Workflow HTTP fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let project = store.create_project(&ctx, "Workflow HTTP", None).unwrap();
    let key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Workflow client",
            KeyScope {
                capabilities: vec![
                    Capability::DocumentsRead,
                    Capability::DocumentsWrite,
                    Capability::DocumentsPublish,
                    Capability::BacklogsWrite,
                    Capability::TasksRead,
                    Capability::TasksWrite,
                    Capability::TasksTransition,
                    Capability::WorkflowsRead,
                    Capability::WorkflowsWrite,
                    Capability::WorkflowsPublish,
                ],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let executor = tokio::runtime::Runtime::new().unwrap();
    let app = executor
        .block_on(async { ApiServer::router(ApiState::new(config, namespace, 4).await.unwrap()) });
    let request = |method: &str, path: &str, token: &str, body: Option<Value>, id: Option<&str>| {
        executor.block_on(async {
            let mut builder = Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .header("mcp-protocol-version", "2025-11-25");
            if let Some(id) = id {
                builder = builder.header("idempotency-key", id);
            }
            let response = app
                .clone()
                .oneshot(
                    builder
                        .body(Body::from(
                            body.map(|v| serde_json::to_vec(&v).unwrap())
                                .unwrap_or_default(),
                        ))
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

    store
        .create_task_type(
            &ctx,
            &project.id,
            &serde_json::from_value(json!({"code":"task","name":"Task"})).unwrap(),
        )
        .unwrap();
    let original=store.create_workflow(&ctx,&project.id,&serde_json::from_value(json!({"name":"Original policy","purpose":"Pinned existing task workflow","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"reason"}]}]}})).unwrap()).unwrap();
    let original = store
        .publish_workflow(
            &ctx,
            &project.id,
            &original.id,
            &serde_json::from_value(
                json!({"expected_revision":original.revision,"reason":"Publish original policy"}),
            )
            .unwrap(),
        )
        .unwrap();
    let target=store.create_workflow(&ctx,&project.id,&serde_json::from_value(json!({"name":"Target policy","purpose":"Explicit mapping required","resource_kinds":["task"],"graph":{"states":[{"id":"queued","name":"Queued","initial":true},{"id":"closed","name":"Closed","terminal":true}],"transitions":[{"id":"close","name":"Close","from":"queued","to":"closed","guards":[{"kind":"reason"}]}]}})).unwrap()).unwrap();
    let target = store
        .publish_workflow(
            &ctx,
            &project.id,
            &target.id,
            &serde_json::from_value(
                json!({"expected_revision":target.revision,"reason":"Publish target default"}),
            )
            .unwrap(),
        )
        .unwrap();
    let source = store
        .create_backlog(
            &ctx,
            &project.id,
            &serde_json::from_value(
                json!({"name":"Source backlog","workflow_version_id":original.id}),
            )
            .unwrap(),
        )
        .unwrap();
    let destination = store
        .create_backlog(
            &ctx,
            &project.id,
            &serde_json::from_value(
                json!({"name":"Destination backlog","workflow_version_id":target.id}),
            )
            .unwrap(),
        )
        .unwrap();
    let task=store.create_task(&ctx,&project.id,&serde_json::from_value(json!({"title":"Preserve and explicitly migrate policy","type_code":"task","backlog_id":source.id,"origin":"Lifecycle protocol fixture"})).unwrap()).unwrap();
    let source = store.get_backlog(&ctx, &project.id, &source.id).unwrap();
    let root = format!("/api/v1/projects/{}/tasks/{}", project.id, task.id);
    let movement = json!({"expected_revision":task.revision,"backlog_id":destination.id,"expected_source_backlog_revision":source.revision,"expected_target_backlog_revision":destination.revision,"reason":"Organize without changing policy"});
    let (status, moved) = request(
        "POST",
        &format!("{root}/move"),
        &key.key,
        Some(movement.clone()),
        Some("move-once"),
    );
    assert_eq!(status, StatusCode::OK, "{moved}");
    assert_eq!(moved["backlog_id"], destination.id);
    assert_eq!(moved["workflow_version_id"], original.id);
    assert_eq!(moved["state_id"], "ready");
    let rpc = |name: &str, args: Value| {
        request(
            "POST",
            "/mcp",
            &key.key,
            Some(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}),
            ),
            None,
        )
    };
    let (_, replay) = rpc(
        "nexora_task_move",
        json!({"project_id":project.id,"task_id":task.id,"request_id":"move-once","movement":movement}),
    );
    assert_eq!(replay["result"]["structuredContent"], moved);
    let mut migration = json!({"expected_revision":moved["revision"],"workflow_version_id":target.id,"state_mapping":{"ready":"queued"},"reason":"Adopt target policy explicitly"});
    assert_eq!(
        request(
            "POST",
            &format!("{root}/workflow-migrations"),
            &key.key,
            Some(migration.clone()),
            Some("missing-map")
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    migration["state_mapping"]["done"] = json!("closed");
    let (_, migrated) = rpc(
        "nexora_task_workflow_migrate",
        json!({"project_id":project.id,"task_id":task.id,"request_id":"migrate-once","migration":migration}),
    );
    let migrated = migrated["result"]["structuredContent"].clone();
    assert_eq!(migrated["workflow_version_id"], target.id);
    assert_eq!(migrated["state_id"], "queued");
    assert_eq!(
        request(
            "POST",
            &format!("{root}/workflow-migrations"),
            &key.key,
            Some(migration),
            Some("migrate-once")
        )
        .1,
        migrated
    );
    let operation = json!({"expected_revision":migrated["revision"],"reason":"Follow the newly pinned workflow"});
    assert_eq!(
        request(
            "POST",
            &format!("{root}/transitions/finish"),
            &key.key,
            Some(operation.clone()),
            Some("old-edge")
        )
        .0,
        StatusCode::CONFLICT
    );
    let (status, closed) = request(
        "POST",
        &format!("{root}/transitions/close"),
        &key.key,
        Some(operation),
        Some("new-edge"),
    );
    assert_eq!(status, StatusCode::OK, "{closed}");
    assert_eq!(closed["state_id"], "closed");
    let (status, history) = request("GET", &format!("{root}/history"), &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    let history = history["items"].as_array().unwrap();
    assert_eq!(
        history
            .iter()
            .filter(|item| item["operation"] == "task.moved")
            .count(),
        1
    );
    assert_eq!(
        history
            .iter()
            .filter(|item| item["operation"] == "task.workflow_migrated")
            .count(),
        1
    );
}
