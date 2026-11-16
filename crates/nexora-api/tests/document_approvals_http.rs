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
#[ignore = "requires PostgreSQL; document authorization read/revoke over HTTP and MCP"]
fn document_approval_reads_and_human_revocations_share_transport_authority() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_doc_approval_http_{}",
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

    let workflow=store.create_workflow(&ctx,&project.id,&serde_json::from_value(json!({"name":"Human document review","purpose":"Explicit revocable approval","resource_kinds":["requirement"],"graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"reviewed","name":"Reviewed","publishable":true}],"transitions":[{"id":"review","name":"Review","from":"draft","to":"reviewed","guards":[{"kind":"reason"},{"kind":"human_approval"}]}]}})).unwrap()).unwrap();
    let flow = store
        .publish_workflow(
            &ctx,
            &project.id,
            &workflow.id,
            &serde_json::from_value(
                json!({"expected_revision":workflow.revision,"reason":"Require human review"}),
            )
            .unwrap(),
        )
        .unwrap();
    let document=store.create_project_document(&ctx,&project.id,&serde_json::from_value(json!({"code":"REQ-APPROVAL","title":"Business rule review","content":{"kind":"requirement","body":{"problem":"Ambiguity","objective":"Declare rule","scope":"Checkout","business_actors":["Customer"],"acceptance_criteria":["Explicit criterion"],"source":"Business owner"}},"origin":"declared_business","workflow_version_id":flow.id,"reason":"Declare requirement"})).unwrap()).unwrap();
    let parameters = json!({"expected_state_revision":1,"reason":"Apply reviewed version"});
    let definition=serde_json::from_value(json!({"target":{"kind":"document_version","document_id":document.id,"version_id":document.latest_version_id,"transition_id":"review","parameters":parameters},"reason":"Request review","expires_in_seconds":3600})).unwrap();
    let pending = store
        .create_approval_request(&ctx, &project.id, &definition)
        .unwrap();
    let approved=store.decide_approval_request(&ctx,&project.id,&pending.id,&serde_json::from_value(json!({"expected_revision":1,"decision":"approved","reason":"Human review","approval_expires_in_seconds":600})).unwrap()).unwrap();
    let id = approved.approval_id.as_deref().unwrap();
    let root = format!(
        "/api/v1/projects/{}/documents/{}/versions/{}",
        project.id, document.id, document.latest_version_id
    );
    let path = format!("{root}/approvals/{id}");
    let (status, authorization) = request("GET", &path, &key.key, None, None);
    assert_eq!(status, StatusCode::OK, "{authorization}");
    assert_eq!(authorization["id"], id);
    assert!(authorization["consumed_at"].is_null());
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
        "nexora_document_approval_get",
        json!({"project_id":project.id,"document_id":document.id,"version_id":document.latest_version_id,"approval_id":id}),
    );
    assert_eq!(read["result"]["structuredContent"], authorization);
    let body = json!({"reason":"Withdraw before execution"});
    let revoke = format!("{path}/revoke");
    assert_eq!(
        request(
            "POST",
            &revoke,
            &key.key,
            Some(body.clone()),
            Some("technical-revoke")
        )
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, revoked) = request(
        "POST",
        &revoke,
        &session.access_token,
        Some(body.clone()),
        Some("human-revoke"),
    );
    assert_eq!(status, StatusCode::OK, "{revoked}");
    assert!(revoked["revoked_at"].is_string());
    assert!(revoked["consumed_at"].is_null());
    assert_eq!(
        request(
            "POST",
            &revoke,
            &session.access_token,
            Some(body),
            Some("human-revoke")
        )
        .1,
        revoked
    );
    let (_, read) = rpc(
        "nexora_document_approval_get",
        json!({"project_id":project.id,"document_id":document.id,"version_id":document.latest_version_id,"approval_id":id}),
    );
    assert_eq!(read["result"]["structuredContent"], revoked);
    let (status, _) = request(
        "POST",
        &format!("{root}/transitions/review"),
        &key.key,
        Some(
            json!({"expected_state_revision":1,"reason":"Apply reviewed version","approval_id":id}),
        ),
        Some("revoked-execution"),
    );
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, version) = request("GET", &root, &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(version["state_revision"], 1);
    assert_eq!(
        request(
            "GET",
            &format!(
                "/api/v1/projects/{}/documents/{}/versions/wrong/approvals/{id}",
                project.id, document.id
            ),
            &key.key,
            None,
            None
        )
        .0,
        StatusCode::NOT_FOUND
    );
}
