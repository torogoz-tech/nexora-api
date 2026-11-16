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
#[ignore = "requires PostgreSQL; requires PostgreSQL; immutable authored requirements and shared HTTP/MCP document operations"]
fn versioned_business_documents_are_shared_by_http_and_mcp() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_documents_http_{}",
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

    let workflow=store.create_workflow(&ctx,&project.id,&serde_json::from_value(json!({"name":"Business review","purpose":"Explicit publication states","resource_kinds":["requirement"],"graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"reviewed","name":"Reviewed","publishable":true}],"transitions":[{"id":"review","name":"Review","from":"draft","to":"reviewed","guards":[{"kind":"reason"}]}]}})).unwrap()).unwrap();
    let flow=store.publish_workflow(&ctx,&project.id,&workflow.id,&serde_json::from_value(json!({"expected_revision":workflow.revision,"reason":"Fixture publication policy"})).unwrap()).unwrap();
    let base = format!("/api/v1/projects/{}/documents", project.id);
    let content = json!({"kind":"requirement","body":{"problem":"Eligibility unclear","objective":"Declare eligibility","scope":"Checkout","business_actors":["Customer"],"acceptance_criteria":["Condition is explicit"],"source":"Business interview"}});
    let definition = json!({"code":"REQ-HTTP","title":"Eligibility","content":content,"origin":"declared_business","workflow_version_id":flow.id,"reason":"Record requirement"});
    let (status, doc) = request(
        "POST",
        &base,
        &key.key,
        Some(definition.clone()),
        Some("create-document"),
    );
    assert_eq!(status, StatusCode::CREATED);
    let id = doc["id"].as_str().unwrap();
    let version = doc["latest_version_id"].as_str().unwrap();
    let (status, catalog) = request(
        "GET",
        &format!("{base}?kind=requirement&limit=1"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(catalog["items"][0]["id"], id);
    assert_eq!(catalog["items"][0]["latest_title"], "Eligibility");
    assert!(catalog["items"][0].get("content").is_none());

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
        "nexora_document_create",
        json!({"project_id":project.id,"request_id":"create-document","definition":definition}),
    );
    assert_eq!(replay["result"]["isError"], false);
    assert_eq!(replay["result"]["structuredContent"], doc);
    let (status, current) = request("GET", &format!("{base}/{id}"), &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(current, doc);
    let (_, read) = rpc(
        "nexora_document_get",
        json!({"project_id":project.id,"document_id":id}),
    );
    assert_eq!(read["result"]["structuredContent"], doc);
    let (status, first) = request(
        "GET",
        &format!("{base}/{id}/versions/{version}"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["content"]["body"]["objective"], "Declare eligibility");
    let publication = json!({"expected_revision":doc["revision"],"version_id":version,"expected_state_revision":first["state_revision"],"reason":"Cannot publish draft"});
    let (status, _) = request(
        "POST",
        &format!("{base}/{id}/publications"),
        &key.key,
        Some(publication),
        Some("draft-publish"),
    );
    assert_eq!(status, StatusCode::CONFLICT);
    let transition = json!({"expected_state_revision":first["state_revision"],"reason":"Business review completed"});
    let (_, reviewed) = rpc(
        "nexora_document_transition",
        json!({"project_id":project.id,"document_id":id,"version_id":version,"transition_id":"review","request_id":"review-once","transition":transition}),
    );
    assert_eq!(reviewed["result"]["isError"], false);
    let reviewed = reviewed["result"]["structuredContent"].clone();
    let (status, replay) = request(
        "POST",
        &format!("{base}/{id}/versions/{version}/transitions/review"),
        &key.key,
        Some(transition),
        Some("review-once"),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay, reviewed);
    let publication = json!({"expected_revision":doc["revision"],"version_id":version,"expected_state_revision":reviewed["state_revision"],"reason":"Publish reviewed requirement"});
    let (_, published) = rpc(
        "nexora_document_publish",
        json!({"project_id":project.id,"document_id":id,"request_id":"publish-once","publication":publication}),
    );
    assert_eq!(published["result"]["isError"], false);
    let (status, replay) = request(
        "POST",
        &format!("{base}/{id}/publications"),
        &key.key,
        Some(publication),
        Some("publish-once"),
    );
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(published["result"]["structuredContent"], replay);
    let (_, first_again) = rpc(
        "nexora_document_version_get",
        json!({"project_id":project.id,"document_id":id,"version_id":version}),
    );
    assert_eq!(
        first_again["result"]["structuredContent"]["checksum"],
        first["checksum"]
    );
    let (_, versions) = rpc(
        "nexora_document_versions_list",
        json!({"project_id":project.id,"document_id":id,"limit":1}),
    );
    assert_eq!(
        versions["result"]["structuredContent"]["items"][0]["id"],
        version
    );
    assert!(
        versions["result"]["structuredContent"]["items"][0]
            .get("content")
            .is_none()
    );
    let (status, _) = request(
        "GET",
        &format!("{base}/{id}/versions?limit=1"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    let (status, export) = request(
        "GET",
        &format!("{base}/{id}/versions/{version}/export?format=markdown"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert!(
        export["content"]
            .as_str()
            .unwrap()
            .contains(first["checksum"].as_str().unwrap())
    );
    let (_, export) = rpc(
        "nexora_document_export",
        json!({"project_id":project.id,"document_id":id,"version_id":version,"format":"json"}),
    );
    assert_eq!(export["result"]["isError"], false);
    let (status, current) = request("GET", &format!("{base}/{id}"), &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    let revision = json!({"expected_revision":current["revision"],"title":"Eligibility clarified","content":content,"origin":"agent_proposed","workflow_version_id":flow.id,"reason":"Propose next revision"});
    let (_, next) = rpc(
        "nexora_document_revise",
        json!({"project_id":project.id,"document_id":id,"request_id":"revise-once","revision":revision}),
    );
    assert_eq!(next["result"]["isError"], false);
    let (status, replay) = request(
        "POST",
        &format!("{base}/{id}/versions"),
        &key.key,
        Some(revision),
        Some("revise-once"),
    );
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(next["result"]["structuredContent"], replay);
    let (status, current) = request("GET", &format!("{base}/{id}"), &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(current["published_version_id"], version);
    let change = json!({"expected_revision":current["revision"],"archived":true,"reason":"Retain historical documentation"});
    let (_, changed) = rpc(
        "nexora_document_change",
        json!({"project_id":project.id,"document_id":id,"request_id":"archive-once","change":change}),
    );
    assert_eq!(changed["result"]["isError"], false);
    let (status, replay) = request(
        "PATCH",
        &format!("{base}/{id}"),
        &key.key,
        Some(change),
        Some("archive-once"),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay, changed["result"]["structuredContent"]);
    let (status, catalog) = request("GET", &base, &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert!(catalog["items"].as_array().unwrap().is_empty());
    let (_, catalog) = rpc(
        "nexora_documents_list",
        json!({"project_id":project.id,"query":{"include_archived":true,"kind":"requirement","limit":1}}),
    );
    assert_eq!(catalog["result"]["isError"], false);
    assert_eq!(catalog["result"]["structuredContent"]["items"][0]["id"], id);
    assert_eq!(
        catalog["result"]["structuredContent"]["items"][0]["latest_title"],
        "Eligibility clarified"
    );
    let (status, events) = request(
        "GET",
        &format!("{base}/{id}/history?limit=1"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(events["items"][0]["event"], "document.created");
    let (_, next_events) = rpc(
        "nexora_document_history",
        json!({"project_id":project.id,"document_id":id,"after":events["next_after"],"limit":100}),
    );
    assert_eq!(next_events["result"]["isError"], false);
    assert_eq!(
        next_events["result"]["structuredContent"]["items"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let (status, publications) = request(
        "GET",
        &format!("{base}/{id}/publications?limit=1"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(publications["items"][0]["version_id"], version);
    let (_, publications_again) = rpc(
        "nexora_document_publications_list",
        json!({"project_id":project.id,"document_id":id,"limit":1}),
    );
    assert_eq!(
        publications_again["result"]["structuredContent"],
        publications
    );
    for suffix in ["?limit=0", "?kind=task", "?unexpected=true"] {
        let (status, _) = request("GET", &format!("{base}{suffix}"), &key.key, None, None);
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
    let (_, denied_catalog) = rpc(
        "nexora_documents_list",
        json!({"project_id":"foreign-project"}),
    );
    assert_eq!(denied_catalog["result"]["isError"], true);

    let (status, _) = request(
        "GET",
        &format!("/api/v1/projects/foreign-project/documents/{id}"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = request(
        "POST",
        &base,
        &key.key,
        Some(json!({"bad":"input"})),
        Some("invalid-document"),
    );
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}
