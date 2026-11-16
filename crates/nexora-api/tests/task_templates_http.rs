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
fn explicit_task_template_versions_are_shared_by_http_and_mcp() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_template_http_{}",
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
    let workflow=store.create_workflow(&ctx,&project.id,&serde_json::from_value(json!({"name":"Delivery","purpose":"Template applications select task flow explicitly","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[]}]}})).unwrap()).unwrap();
    let flow = store
        .publish_workflow(
            &ctx,
            &project.id,
            &workflow.id,
            &serde_json::from_value(
                json!({"expected_revision":workflow.revision,"reason":"Fixture"}),
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
    let base = format!("/api/v1/projects/{}/task-templates", project.id);
    let (status, template) = request(
        "POST",
        &base,
        &key.key,
        Some(json!({"code":"triage","name":"Incident triage"})),
        Some("create-template"),
    );
    assert_eq!(status, StatusCode::CREATED);
    let id = template["id"].as_str().unwrap();
    let (status, read) = request("GET", &format!("{base}/{id}"), &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["code"], "triage");
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
    let publication = json!({"expected_revision":template["revision"],"definition":{"title":"Investigate incident","context":"Capture reproduction and impact","type_code":"task","priority":"high","fields":{"syntax":"Map<K,V>"}},"reason":"Explicit defaults"});
    let (_, published) = rpc(
        "nexora_task_template_publish",
        json!({"project_id":project.id,"template_id":id,"request_id":"publish-template","publication":publication}),
    );
    assert_eq!(published["result"]["isError"], false);
    let version = published["result"]["structuredContent"].clone();
    let (status, replay) = request(
        "POST",
        &format!("{base}/{id}/versions"),
        &key.key,
        Some(publication),
        Some("publish-template"),
    );
    assert_eq!(status, StatusCode::CREATED);
    assert!(replay == version);
    let application = json!({"template_version_id":version["id"],"title":"Customer incident","workflow_version_id":flow.id,"origin":"External developer"});
    let (_, applied) = rpc(
        "nexora_task_template_apply",
        json!({"project_id":project.id,"request_id":"apply-template","application":application}),
    );
    assert_eq!(applied["result"]["isError"], false);
    let task = applied["result"]["structuredContent"].clone();
    assert_eq!(task["template_version_id"], version["id"]);
    assert_eq!(task["context"], "Capture reproduction and impact");
    assert_eq!(task["template_snapshot"]["checksum"], version["checksum"]);
    let (status, replay) = request(
        "POST",
        &format!("{base}/apply"),
        &key.key,
        Some(application.clone()),
        Some("apply-template"),
    );
    assert_eq!(status, StatusCode::CREATED);
    assert!(replay == task);
    let (_, versions) = rpc(
        "nexora_task_template_versions_list",
        json!({"project_id":project.id,"template_id":id,"limit":1}),
    );
    assert_eq!(
        versions["result"]["structuredContent"]["items"][0]["id"],
        version["id"]
    );
    assert!(
        versions["result"]["structuredContent"]["items"][0]
            .get("definition")
            .is_none()
    );
    let (_, historical) = rpc(
        "nexora_task_template_version_get",
        json!({"project_id":project.id,"template_id":id,"version_id":version["id"]}),
    );
    assert_eq!(
        historical["result"]["structuredContent"]["checksum"],
        version["checksum"]
    );
    let (status, page) = request(
        "GET",
        &format!("{base}/{id}/versions?limit=1"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["items"][0]["id"], version["id"]);
    let (status, historical) = request(
        "GET",
        &format!("{base}/{id}/versions/{}", version["id"].as_str().unwrap()),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(historical["checksum"], version["checksum"]);
    let (_, current) = rpc(
        "nexora_task_template_get",
        json!({"project_id":project.id,"template_id":id}),
    );
    let revision = current["result"]["structuredContent"]["revision"].clone();
    let (status, changed) = request(
        "PATCH",
        &format!("{base}/{id}"),
        &key.key,
        Some(
            json!({"expected_revision":revision,"name":"Triage revised","reason":"Display metadata only"}),
        ),
        Some("rename-template"),
    );
    assert_eq!(status, StatusCode::OK);
    let change = json!({"expected_revision":changed["revision"],"archived":true,"reason":"Retire defaults, retain evidence"});
    let (_, archived) = rpc(
        "nexora_task_template_change",
        json!({"project_id":project.id,"template_id":id,"request_id":"archive-template","change":change}),
    );
    assert_eq!(archived["result"]["isError"], false);
    let (status, replay) = request(
        "PATCH",
        &format!("{base}/{id}"),
        &key.key,
        Some(change),
        Some("archive-template"),
    );
    assert_eq!(status, StatusCode::OK);
    assert!(replay == archived["result"]["structuredContent"]);
    let (status, _) = request(
        "POST",
        &format!("{base}/apply"),
        &key.key,
        Some(application.clone()),
        Some("new-archived-application"),
    );
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, replay) = request(
        "POST",
        &format!("{base}/apply"),
        &key.key,
        Some(application),
        Some("apply-template"),
    );
    assert_eq!(status, StatusCode::CREATED);
    assert!(replay == task);
    let (_, created) = rpc(
        "nexora_task_template_create",
        json!({"project_id":project.id,"request_id":"auxiliary-template","definition":{"code":"auxiliary","name":"Another explicit template"}}),
    );
    assert_eq!(created["result"]["isError"], false);
    let (_, templates) = rpc(
        "nexora_task_templates_list",
        json!({"project_id":project.id,"limit":10}),
    );
    assert_eq!(
        templates["result"]["structuredContent"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (status, templates) = request(
        "GET",
        &format!("{base}?include_archived=true&limit=10"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(templates["items"].as_array().unwrap().len(), 2);
    let (_, denied) = rpc(
        "nexora_task_template_get",
        json!({"project_id":"foreign-project","template_id":id}),
    );
    assert_eq!(denied["result"]["isError"], true);
}
