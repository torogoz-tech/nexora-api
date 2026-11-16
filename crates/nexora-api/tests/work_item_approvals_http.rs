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
#[ignore = "requires PostgreSQL; task REST and MCP share persistent workflow operations and exact retries"]
fn human_approval_http_is_readable_by_mcp_and_revocable_before_consumption() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_approval_http_{}",
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
                    Capability::ApprovalsGrant,
                    Capability::TasksRead,
                    Capability::TasksWrite,
                    Capability::TasksTransition,
                    Capability::BacklogsWrite,
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
    let workflow=store.create_workflow(&ctx,&project.id,&serde_json::from_value(json!({"name":"Reviewed delivery","purpose":"Exact human approval protocol","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"human_approval"}]}]}})).unwrap()).unwrap();
    let version = store
        .publish_workflow(
            &ctx,
            &project.id,
            &workflow.id,
            &serde_json::from_value(
                json!({"expected_revision":workflow.revision,"reason":"Protocol fixture"}),
            )
            .unwrap(),
        )
        .unwrap();
    store
        .create_task_type(
            &ctx,
            &project.id,
            &serde_json::from_value(json!({"code":"bug","name":"Bug"})).unwrap(),
        )
        .unwrap();
    let task=store.create_task(&ctx,&project.id,&serde_json::from_value(json!({"title":"Review exact operation","type_code":"bug","workflow_version_id":version.id,"origin":"External developer"})).unwrap()).unwrap();
    let path = format!(
        "/api/v1/projects/{}/tasks/{}/approvals",
        project.id, task.id
    );
    let parameters = json!({"expected_revision":task.revision,"resolution":"Reviewed solution"});
    let grant = json!({"transition_id":"finish","parameters":parameters,"reason":"Human review","expires_in_seconds":3600});
    let (status, _) = request(
        "POST",
        &path,
        &key.key,
        Some(grant.clone()),
        Some("technical-grant"),
    );
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, approval) = request(
        "POST",
        &path,
        &session.access_token,
        Some(grant.clone()),
        Some("human-grant"),
    );
    assert_eq!(status, StatusCode::CREATED);
    let approval_id = approval["id"].as_str().unwrap();
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
    let (_, read) = rpc(
        "nexora_task_approval_get",
        json!({"project_id":project.id,"task_id":task.id,"approval_id":approval_id}),
    );
    assert_eq!(read["result"]["structuredContent"]["id"], approval["id"]);
    let (status, read) = request(
        "GET",
        &format!("{path}/{approval_id}"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["parameters"]["resolution"], "Reviewed solution");
    let revoke = json!({"reason":"Approval withdrawn before execution"});
    let (status, _) = request(
        "POST",
        &format!("{path}/{approval_id}/revoke"),
        &key.key,
        Some(revoke.clone()),
        Some("technical-revoke"),
    );
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, revoked) = request(
        "POST",
        &format!("{path}/{approval_id}/revoke"),
        &session.access_token,
        Some(revoke),
        Some("human-revoke"),
    );
    assert_eq!(status, StatusCode::OK);
    assert!(!revoked["revoked_at"].is_null());
    let mut execute = parameters.clone();
    execute["approval_id"] = json!(approval_id);
    let (_, denied) = rpc(
        "nexora_task_transition",
        json!({"project_id":project.id,"task_id":task.id,"transition_id":"finish","request_id":"revoked-transition","parameters":execute}),
    );
    assert_eq!(denied["result"]["isError"], true);
    let (status, new_approval) = request(
        "POST",
        &path,
        &session.access_token,
        Some(grant),
        Some("new-human-grant"),
    );
    assert_eq!(status, StatusCode::CREATED);
    execute["approval_id"] = new_approval["id"].clone();
    let (_, finished) = rpc(
        "nexora_task_transition",
        json!({"project_id":project.id,"task_id":task.id,"transition_id":"finish","request_id":"approved-transition","parameters":execute}),
    );
    assert_eq!(finished["result"]["structuredContent"]["state_id"], "done");
    let (_, read) = rpc(
        "nexora_task_approval_get",
        json!({"project_id":project.id,"task_id":task.id,"approval_id":new_approval["id"]}),
    );
    assert!(!read["result"]["structuredContent"]["consumed_at"].is_null());
}
