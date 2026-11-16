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
#[ignore = "requires PostgreSQL; approval lifecycle and transport parity"]
fn approval_requests_share_receipts_and_require_human_session_decisions() {
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
    let workflow=store.create_workflow(&ctx,&project.id,&serde_json::from_value(json!({"name":"Human delivery","purpose":"Human decision transport parity","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"reason"},{"kind":"human_approval"}]}]}})).unwrap()).unwrap();
    let flow = store
        .publish_workflow(
            &ctx,
            &project.id,
            &workflow.id,
            &serde_json::from_value(
                json!({"expected_revision":workflow.revision,"reason":"Require a human session"}),
            )
            .unwrap(),
        )
        .unwrap();
    let task=store.create_task(&ctx,&project.id,&serde_json::from_value(json!({"title":"Controlled operation","type_code":"task","workflow_version_id":flow.id,"origin":"Protocol fixture"})).unwrap()).unwrap();
    let root = format!("/api/v1/projects/{}/approval-requests", project.id);
    let definition = json!({"target":{"kind":"task","task_id":task.id,"transition_id":"finish","parameters":{"expected_revision":task.revision,"reason":"Execute reviewed operation"}},"reason":"Ask for human review","expires_in_seconds":3600});
    assert_eq!(
        request("POST", &root, &key.key, Some(definition.clone()), None).0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, created) = request(
        "POST",
        &root,
        &key.key,
        Some(definition.clone()),
        Some("request-once"),
    );
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap();
    let path = format!("{root}/{id}");
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
        "nexora_approval_request_create",
        json!({"project_id":project.id,"request_id":"request-once","definition":definition}),
    );
    assert_eq!(replay["result"]["structuredContent"], created);
    let (_, listed) = rpc(
        "nexora_approval_requests_list",
        json!({"project_id":project.id,"status":"pending","limit":1}),
    );
    assert_eq!(listed["result"]["structuredContent"]["items"][0]["id"], id);
    let decision = json!({"expected_revision":created["revision"],"decision":"approved","reason":"Human reviewed exact operation","approval_expires_in_seconds":600});
    assert_eq!(
        request(
            "POST",
            &format!("{path}/decisions"),
            &key.key,
            Some(decision.clone()),
            Some("technical-decision")
        )
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, approved) = request(
        "POST",
        &format!("{path}/decisions"),
        &session.access_token,
        Some(decision.clone()),
        Some("human-decision"),
    );
    assert_eq!(status, StatusCode::OK, "{approved}");
    assert_eq!(approved["status"], "approved");
    assert_eq!(
        request(
            "POST",
            &format!("{path}/decisions"),
            &session.access_token,
            Some(decision),
            Some("human-decision")
        )
        .1,
        approved
    );
    let (_, read) = rpc(
        "nexora_approval_request_get",
        json!({"project_id":project.id,"approval_request_id":id}),
    );
    assert_eq!(read["result"]["structuredContent"], approved);
    let operation = json!({"expected_revision":task.revision,"reason":"Execute reviewed operation","approval_id":approved["approval_id"]});
    let transition = format!(
        "/api/v1/projects/{}/tasks/{}/transitions/finish",
        project.id, task.id
    );
    let (status, done) = request(
        "POST",
        &transition,
        &key.key,
        Some(operation),
        Some("execute-approved"),
    );
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["state_id"], "done");
    // A separate pending request can be withdrawn through MCP and replayed through HTTP.
    let next=store.create_task(&ctx,&project.id,&serde_json::from_value(json!({"title":"Withdrawn operation","type_code":"task","workflow_version_id":flow.id,"origin":"Protocol fixture"})).unwrap()).unwrap();
    let mut other = definition;
    other["target"]["task_id"] = json!(next.id);
    let (status, pending) = request(
        "POST",
        &root,
        &key.key,
        Some(other),
        Some("request-withdraw"),
    );
    assert_eq!(status, StatusCode::CREATED);
    let pending_id = pending["id"].as_str().unwrap();
    let cancellation = json!({"expected_revision":pending["revision"],"reason":"No longer needed"});
    let (_, cancelled) = rpc(
        "nexora_approval_request_cancel",
        json!({"project_id":project.id,"approval_request_id":pending_id,"request_id":"withdraw-once","cancellation":cancellation}),
    );
    let cancelled = cancelled["result"]["structuredContent"].clone();
    assert_eq!(cancelled["status"], "cancelled");
    assert_eq!(
        request(
            "POST",
            &format!("{root}/{pending_id}/cancel"),
            &key.key,
            Some(cancellation),
            Some("withdraw-once")
        )
        .1,
        cancelled
    );
    let (status, page) = request(
        "GET",
        &format!("{root}?status=approved&limit=1"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["items"][0]["id"], id);
    assert_eq!(
        request("GET", &format!("{root}?limit=0"), &key.key, None, None).0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
}
