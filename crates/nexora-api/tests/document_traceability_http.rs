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
#[ignore = "requires PostgreSQL; requires PostgreSQL; explicit traceability and immutable context packages over HTTP/MCP"]
fn document_links_and_context_packages_share_http_and_mcp_receipts() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_trace_http_{}",
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

    let workflow=store.create_workflow(&ctx,&project.id,&serde_json::from_value(json!({"name":"Explicit publication policy","purpose":"Initial state is explicitly publishable in this fixture","resource_kinds":["requirement"],"graph":{"states":[{"id":"reviewed","name":"Reviewed","initial":true,"publishable":true}],"transitions":[{"id":"confirm","name":"Confirm","from":"reviewed","to":"reviewed","guards":[{"kind":"reason"}]}]}})).unwrap()).unwrap();
    let flow = store
        .publish_workflow(
            &ctx,
            &project.id,
            &workflow.id,
            &serde_json::from_value(
                json!({"expected_revision":workflow.revision,"reason":"Explicit fixture policy"}),
            )
            .unwrap(),
        )
        .unwrap();
    store
        .create_task_type(
            &ctx,
            &project.id,
            &serde_json::from_value(json!({"code":"task","name":"Task"})).unwrap(),
        )
        .unwrap();
    let task_workflow=store.create_workflow(&ctx,&project.id,&serde_json::from_value(json!({"name":"Required document delivery","purpose":"Server verified document prerequisite","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"approved_document_version"}]}]}})).unwrap()).unwrap();
    let task_flow=store.publish_workflow(&ctx,&project.id,&task_workflow.id,&serde_json::from_value(json!({"expected_revision":task_workflow.revision,"reason":"Require real approved links"})).unwrap()).unwrap();
    let task=store.create_task(&ctx,&project.id,&serde_json::from_value(json!({"title":"Implement business rule","type_code":"task","workflow_version_id":task_flow.id,"origin":"External developer","fields":{"approved_document_version":true}})).unwrap()).unwrap();
    let docs = format!("/api/v1/projects/{}/documents", project.id);
    let content = json!({"kind":"requirement","body":{"problem":"Unclear rule","objective":"Declare business conditions","scope":"Checkout","business_actors":["Customer"],"acceptance_criteria":["Condition is explicit"],"source":"Business owner"}});
    let (status, doc) = request(
        "POST",
        &docs,
        &key.key,
        Some(
            json!({"code":"REQ-LINK","title":"Eligibility","content":content,"origin":"declared_business","workflow_version_id":flow.id,"reason":"Capture business requirement"}),
        ),
        Some("create-doc"),
    );
    assert_eq!(status, StatusCode::CREATED);
    let id = doc["id"].as_str().unwrap();
    let version = doc["latest_version_id"].as_str().unwrap();
    let links = format!("{docs}/{id}/links");
    let link = json!({"expected_document_revision":doc["revision"],"version_id":version,"target":{"kind":"task","task_id":task.id,"expected_revision":task.revision},"kind":"required_for_transition","reason":"Bind an exact prerequisite"});
    let (status, created) = request(
        "POST",
        &links,
        &key.key,
        Some(link.clone()),
        Some("link-once"),
    );
    assert_eq!(status, StatusCode::CREATED);
    let link_id = created["id"].as_str().unwrap();
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
        "nexora_document_link_create",
        json!({"project_id":project.id,"document_id":id,"request_id":"link-once","link":link}),
    );
    assert_eq!(replay["result"]["structuredContent"], created);
    let (status, page) = request("GET", &links, &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["items"][0]["id"], link_id);
    let (_, page) = rpc(
        "nexora_document_links_list",
        json!({"project_id":project.id,"document_id":id,"limit":1}),
    );
    assert_eq!(
        page["result"]["structuredContent"]["items"][0]["id"],
        link_id
    );
    let task_docs = format!(
        "/api/v1/projects/{}/tasks/{}/documents",
        project.id, task.id
    );
    let (status, page) = request("GET", &task_docs, &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["items"][0]["id"], link_id);
    let (_, page) = rpc(
        "nexora_task_documents_list",
        json!({"project_id":project.id,"task_id":task.id,"limit":1}),
    );
    assert_eq!(
        page["result"]["structuredContent"]["items"][0]["source_version_id"],
        version
    );
    let current = store.get_project_document(&ctx, &project.id, id).unwrap();
    let (status, _) = request(
        "POST",
        &format!("{docs}/{id}/publications"),
        &key.key,
        Some(
            json!({"expected_revision":current.revision,"version_id":version,"expected_state_revision":1,"reason":"Publish explicitly eligible version"}),
        ),
        Some("publish-doc"),
    );
    assert_eq!(status, StatusCode::CREATED);
    let task = store.get_task(&ctx, &project.id, &task.id).unwrap();
    let (status, finished) = request(
        "POST",
        &format!(
            "/api/v1/projects/{}/tasks/{}/transitions/finish",
            project.id, task.id
        ),
        &key.key,
        Some(json!({"expected_revision":task.revision,"reason":"Real prerequisite now approved"})),
        Some("finish-task"),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(finished["state_id"], "done");
    let packages = format!("/api/v1/projects/{}/context-packages", project.id);
    let selection = json!({"name":"Developer context","document_versions":[{"document_id":id,"version_id":version}],"task_ids":[task.id],"reason":"Capture explicit work and business context"});
    let (_, captured) = rpc(
        "nexora_context_package_create",
        json!({"project_id":project.id,"request_id":"package-once","selection":selection}),
    );
    assert_eq!(captured["result"]["isError"], false);
    let package = captured["result"]["structuredContent"].clone();
    let package_id = package["id"].as_str().unwrap();
    let (status, replay) = request(
        "POST",
        &packages,
        &key.key,
        Some(selection),
        Some("package-once"),
    );
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(replay, package);
    let (status, read) = request(
        "GET",
        &format!("{packages}/{package_id}"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["needs_review"], false);
    let (_, read) = rpc(
        "nexora_context_package_get",
        json!({"project_id":project.id,"package_id":package_id}),
    );
    assert_eq!(
        read["result"]["structuredContent"]["checksum"],
        package["checksum"]
    );
    let (status, export) = request(
        "GET",
        &format!("{packages}/{package_id}/export?format=json"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert!(
        export["content"]
            .as_str()
            .unwrap()
            .contains("Declare business conditions")
    );
    let (_, markdown) = rpc(
        "nexora_context_package_export",
        json!({"project_id":project.id,"package_id":package_id,"format":"markdown"}),
    );
    assert_eq!(markdown["result"]["isError"], false);
    assert!(
        markdown["result"]["structuredContent"]["content"]
            .as_str()
            .unwrap()
            .contains("Implement business rule")
    );
    let current = store.get_project_document(&ctx, &project.id, id).unwrap();
    let task = store.get_task(&ctx, &project.id, &task.id).unwrap();
    let removal = json!({"expected_document_revision":current.revision,"expected_target_revision":task.revision,"reason":"Retain historical link"});
    let (_, removed) = rpc(
        "nexora_document_link_remove",
        json!({"project_id":project.id,"document_id":id,"link_id":link_id,"request_id":"remove-once","removal":removal}),
    );
    assert_eq!(removed["result"]["isError"], false);
    let (status, replay) = request(
        "POST",
        &format!("{links}/{link_id}/remove"),
        &key.key,
        Some(removal),
        Some("remove-once"),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay, removed["result"]["structuredContent"]);
    let (status, active) = request("GET", &links, &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert!(active["items"].as_array().unwrap().is_empty());
    let (status, historical) = request(
        "GET",
        &format!("{links}?include_removed=true"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(historical["items"].as_array().unwrap().len(), 1);
    let (status, stale) = request(
        "GET",
        &format!("{packages}/{package_id}"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(stale["needs_review"], true);
    let (status, export_after) = request(
        "GET",
        &format!("{packages}/{package_id}/export?format=json"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(export["content"], export_after["content"]);
    let limited = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Document-only package reader",
            KeyScope {
                capabilities: vec![Capability::DocumentsRead],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let (status, _) = request(
        "GET",
        &format!("{packages}/{package_id}"),
        &limited.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = request(
        "GET",
        &format!("/api/v1/projects/foreign-project/context-packages/{package_id}"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::NOT_FOUND);
}
