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
#[ignore = "requires PostgreSQL; workflow REST and MCP share versioning, permissions and exact retry services"]
fn workflow_http_and_mcp_publish_immutable_versions_without_project_scope() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_workflow_http_{}",
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
    let gate = store
        .identity_gate(&session.access_token, "skip", "")
        .unwrap();
    let access_token = gate["access_token"].as_str().unwrap();
    let ctx = store.authenticate(access_token).unwrap();
    let key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Workflow client",
            KeyScope {
                capabilities: vec![
                    Capability::WorkflowsRead,
                    Capability::WorkflowsWrite,
                    Capability::WorkflowsPublish,
                ],
                project_ids: vec![],
                all_projects: false,
                installation: true,
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
    let base = "/api/v1/workflows";
    let graph = json!({"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[]}]});
    let input = json!({"name":"Delivery","purpose":"Explicit governing workflow","graph":graph});
    let (status, created) = request(
        "POST",
        base,
        access_token,
        Some(input.clone()),
        Some("create-workflow"),
    );
    assert_eq!(status, StatusCode::CREATED);
    let (_, replayed) = request(
        "POST",
        base,
        access_token,
        Some(input),
        Some("create-workflow"),
    );
    assert_eq!(replayed["id"], created["id"]);
    let id = created["id"].as_str().unwrap();
    let publish = json!({"expected_revision":created["revision"],"reason":"Initial publication"});
    let (status, published) = request(
        "POST",
        &format!("{base}/{id}/publish"),
        access_token,
        Some(publish),
        Some("publish-workflow"),
    );
    assert_eq!(status, StatusCode::CREATED);
    let version = published["id"].as_str().unwrap();
    let (status, historical) = request(
        "GET",
        &format!("{base}/{id}/versions/{version}"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(historical["checksum"], published["checksum"]);
    let (status, versions) = request(
        "GET",
        &format!("{base}/{id}/versions?limit=1"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(versions["items"][0]["id"], published["id"]);
    assert!(versions["items"][0].get("graph").is_none());
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
    let (_, reply) = rpc("nexora_workflows_list", json!({"limit":1}));
    assert_eq!(reply["result"]["isError"], false);
    assert_eq!(
        reply["result"]["structuredContent"]["items"][0]["id"],
        created["id"]
    );
    let (_, reply) = rpc("nexora_workflow_get", json!({"workflow_id":id}));
    assert_eq!(
        reply["result"]["structuredContent"]["latest_version_id"],
        published["id"]
    );
    let current = reply["result"]["structuredContent"]["revision"].clone();
    let (_, changed) = rpc(
        "nexora_workflow_change",
        json!({"workflow_id":id,"request_id":"archive-workflow","change":{"expected_revision":current,"archived":true,"reason":"Retain history"}}),
    );
    assert_eq!(changed["result"]["isError"], false);
    let (_, history) = rpc(
        "nexora_workflow_version_get",
        json!({"workflow_id":id,"version_id":version}),
    );
    assert_eq!(
        history["result"]["structuredContent"]["checksum"],
        published["checksum"]
    );
    let (_, denied) = rpc(
        "nexora_workflow_get",
        json!({"workflow_id":"missing-workflow"}),
    );
    assert_eq!(denied["result"]["isError"], true);
    let (_, new_flow) = rpc(
        "nexora_workflow_create",
        json!({"request_id":"mcp-create","definition":{"name":"MCP flow","purpose":"Same service through MCP","graph":graph}}),
    );
    assert_eq!(new_flow["result"]["isError"], false);
    let new_id = new_flow["result"]["structuredContent"]["id"]
        .as_str()
        .unwrap();
    let (_, new_version) = rpc(
        "nexora_workflow_publish",
        json!({"workflow_id":new_id,"request_id":"mcp-publish","publish":{"expected_revision":new_flow["result"]["structuredContent"]["revision"],"reason":"MCP publication"}}),
    );
    assert_eq!(new_version["result"]["isError"], false);
    let (_, history_page) = rpc(
        "nexora_workflow_versions_list",
        json!({"workflow_id":new_id,"limit":1}),
    );
    assert_eq!(
        history_page["result"]["structuredContent"]["items"][0]["id"],
        new_version["result"]["structuredContent"]["id"]
    );
    let (_, catalog) = request(
        "POST",
        "/mcp",
        &key.key,
        Some(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})),
        None,
    );
    let names = catalog["result"]["tools"].as_array().unwrap();
    for name in [
        "nexora_workflows_list",
        "nexora_workflow_get",
        "nexora_workflow_create",
        "nexora_workflow_change",
        "nexora_workflow_publish",
        "nexora_workflow_version_get",
        "nexora_workflow_versions_list",
    ] {
        assert!(names.iter().any(|tool| tool["name"] == name));
    }
    let actor = ctx.actor_id();
    let member_path = format!("/api/v1/actors/{actor}/workflow");
    let binding = json!({"expected_revision":0,"workflow_version_id":new_version["result"]["structuredContent"]["id"],"reason":"Join developer workflow"});
    let (status, assigned) = request(
        "PUT",
        &member_path,
        access_token,
        Some(binding.clone()),
        Some("bind-developer"),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(assigned["workflow_id"], new_id);
    let (_, replayed) = request(
        "PUT",
        &member_path,
        access_token,
        Some(binding.clone()),
        Some("bind-developer"),
    );
    assert_eq!(assigned, replayed);
    let (status, _) = request(
        "PUT",
        &member_path,
        access_token,
        Some(binding),
        Some("stale-developer"),
    );
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, members) = request(
        "GET",
        &format!("/api/v1/workflows/{new_id}/developers"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(members.as_array().unwrap().len(), 1);
    let (_, mine) = rpc("nexora_developer_workflow_get", json!({}));
    assert_eq!(mine["result"]["structuredContent"]["workflow_id"], new_id);
    let (_, developers) = rpc(
        "nexora_workflow_developers_list",
        json!({"workflow_id":new_id}),
    );
    assert_eq!(developers["result"]["isError"], false);
    let (_, board) = rpc("nexora_developer_tasks_list", json!({"limit":10}));
    assert_eq!(board["result"]["isError"], false);
    let project = store
        .create_project(&ctx, "Developer project one", None)
        .unwrap();
    let project_two = store
        .create_project(&ctx, "Developer project two", None)
        .unwrap();
    for project in [&project, &project_two] {
        store
            .create_task_type(
                &ctx,
                &project.id,
                &nexora_platform::TaskTypeCreate {
                    purpose: nexora_platform::TaskPurpose::Implementation,
                    code: "delivery".into(),
                    name: "Task".into(),
                    description: None,
                    color: None,
                },
            )
            .unwrap();
    }
    let task_input = json!({"title":"Shared developer flow","type_code":"delivery","entry_workflow_id":new_id,"origin":"HTTP test"});
    let (status, task) = request(
        "POST",
        &format!("/api/v1/projects/{}/tasks", project.id),
        access_token,
        Some(task_input.clone()),
        Some("global-task-one"),
    );
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(task["workflow_id"], Value::Null);
    assert_eq!(task["lifecycle"], "pending");
    let (status, taken) = request(
        "POST",
        &format!("/api/v1/tasks/{}/take", task["id"].as_str().unwrap()),
        access_token,
        Some(json!({"expected_revision":task["revision"]})),
        Some("take-task-one"),
    );
    assert_eq!(status, StatusCode::OK, "{taken}");
    assert_eq!(taken["workflow_id"], new_id);
    assert_eq!(taken["assignee_id"], ctx.actor_id());
    let (status, task_two) = request(
        "POST",
        &format!("/api/v1/projects/{}/tasks", project_two.id),
        access_token,
        Some(task_input),
        Some("global-task-two"),
    );
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(task_two["workflow_id"], Value::Null);
    let (status, taken) = request(
        "POST",
        &format!("/api/v1/tasks/{}/take", task_two["id"].as_str().unwrap()),
        access_token,
        Some(json!({"expected_revision":task_two["revision"]})),
        Some("take-task-two"),
    );
    assert_eq!(status, StatusCode::OK, "{taken}");
    assert_eq!(taken["workflow_id"], new_id);
    let (status, board) = request("GET", "/api/v1/me/workflow-tasks", access_token, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(board["items"].as_array().unwrap().len(), 2);
    let (status, limited_board) = request("GET", "/api/v1/me/workflow-tasks", &key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert!(limited_board["items"].as_array().unwrap().is_empty());
    let scoped = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Project scope only",
            KeyScope {
                capabilities: vec![Capability::WorkflowsRead],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let (status, scoped_workflows) = request(
        "GET",
        "/api/v1/workflows?include_archived=true",
        &scoped.key,
        None,
        None,
    );
    assert_eq!(scoped_workflows["items"].as_array().unwrap().len(), 1);
    assert_eq!(scoped_workflows["items"][0]["id"], new_id);
    assert_eq!(status, StatusCode::OK);
    let (status, _) = request(
        "POST",
        "/api/v1/workflows",
        access_token,
        Some(json!({"name":"Invalid scoped workflow","project_id":project.id,"graph":graph})),
        Some("reject-project-field"),
    );
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, old_link) = request(
        "GET",
        &format!(
            "/api/v1/projects/{}/workflows/{new_id}/versions/{}",
            project.id,
            new_version["result"]["structuredContent"]["id"]
                .as_str()
                .unwrap()
        ),
        access_token,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(old_link["workflow_id"], new_id);
}
