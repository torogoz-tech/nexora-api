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
fn task_http_and_mcp_share_workflow_state_receipts_and_history() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_tasks_http_{}",
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
    let workflow=store.create_workflow(&ctx,&project.id,&serde_json::from_value(json!({"name":"Delivery","purpose":"Task protocol fixture","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[]}]}})).unwrap()).unwrap();
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
    let base = format!("/api/v1/projects/{}", project.id);
    let (status, _) = request(
        "POST",
        &format!("{base}/task-types"),
        &key.key,
        Some(json!({"code":"bug","name":"Bug","description":null})),
        Some("create-type"),
    );
    assert_eq!(status, StatusCode::CREATED);
    let (status, backlog) = request(
        "POST",
        &format!("{base}/backlogs"),
        &key.key,
        Some(json!({"name":"Delivery","description":null,"workflow_version_id":version.id})),
        Some("create-backlog"),
    );
    assert_eq!(status, StatusCode::CREATED);
    let input = json!({"title":"Protocol bug","context":"Investigate","type_code":"bug","backlog_id":backlog["id"],"origin":"External developer"});
    let (status, task) = request(
        "POST",
        &format!("{base}/tasks"),
        &key.key,
        Some(input.clone()),
        Some("create-task"),
    );
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(task["state_id"], "ready");
    let (_, replay) = request(
        "POST",
        &format!("{base}/tasks"),
        &key.key,
        Some(input.clone()),
        Some("create-task"),
    );
    assert_eq!(task["id"], replay["id"]);
    let (status, _) = request(
        "POST",
        &format!("{base}/tasks"),
        &key.key,
        Some(input),
        None,
    );
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let id = task["id"].as_str().unwrap();
    let (status, _) = request(
        "PATCH",
        &format!("{base}/tasks/{id}"),
        &key.key,
        Some(json!({"expected_revision":1,"state_id":"done","reason":"Cannot forge state"})),
        Some("invalid-state"),
    );
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
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
    let (_, got) = rpc(
        "nexora_task_get",
        json!({"project_id":project.id,"task_id":id}),
    );
    assert_eq!(got["result"]["structuredContent"]["id"], task["id"]);
    let (_, changed) = rpc(
        "nexora_task_change",
        json!({"project_id":project.id,"task_id":id,"request_id":"assign-priority","change":{"expected_revision":task["revision"],"priority":"high","reason":"Investigation priority"}}),
    );
    assert_eq!(changed["result"]["isError"], false);
    let revision = changed["result"]["structuredContent"]["revision"].clone();
    let parameters = json!({"expected_revision":revision,"resolution":"Verified"});
    let (_, finished) = rpc(
        "nexora_task_transition",
        json!({"project_id":project.id,"task_id":id,"transition_id":"finish","request_id":"finish-task","parameters":parameters}),
    );
    assert_eq!(finished["result"]["structuredContent"]["state_id"], "done");
    let (status, replay) = request(
        "POST",
        &format!("{base}/tasks/{id}/transitions/finish"),
        &key.key,
        Some(parameters.clone()),
        Some("finish-task"),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        replay["revision"], finished["result"]["structuredContent"]["revision"],
        "REST/MCP use the same canonical receipt input"
    );
    let mut altered_parameters = parameters.clone();
    altered_parameters["expected_revision"] = json!(revision.as_i64().unwrap() + 1);
    let (status, altered_replay) = request(
        "POST",
        &format!("{base}/tasks/{id}/transitions/finish"),
        &key.key,
        Some(altered_parameters),
        Some("finish-task"),
    );
    assert_eq!(status, StatusCode::CONFLICT, "{altered_replay}");
    let (_, history) = rpc(
        "nexora_task_history",
        json!({"project_id":project.id,"task_id":id,"limit":50}),
    );
    assert_eq!(
        history["result"]["structuredContent"]["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let (_, listed) = rpc(
        "nexora_tasks_list",
        json!({"project_id":project.id,"query":{"backlog_id":backlog["id"],"priority":"high"}}),
    );
    assert_eq!(
        listed["result"]["structuredContent"]["items"][0]["id"],
        task["id"]
    );
    assert!(
        listed["result"]["structuredContent"]["items"][0]
            .get("context")
            .is_none()
    );
    let (_, denied) = rpc(
        "nexora_task_get",
        json!({"project_id":"foreign-project","task_id":id}),
    );
    assert_eq!(denied["result"]["isError"], true);
    let (_, new_type) = rpc(
        "nexora_task_type_create",
        json!({"project_id":project.id,"request_id":"mcp-work-type","definition":{"code":"improvement","name":"Improvement"}}),
    );
    assert_eq!(
        new_type["result"]["structuredContent"]["code"],
        "improvement"
    );
    let (_, new_backlog) = rpc(
        "nexora_backlog_create",
        json!({"project_id":project.id,"request_id":"mcp-backlog","definition":{"name":"Planning","workflow_version_id":version.id}}),
    );
    assert_eq!(new_backlog["result"]["isError"], false);
    let planning = new_backlog["result"]["structuredContent"]["id"]
        .as_str()
        .unwrap();
    let (_, changed_backlog) = rpc(
        "nexora_backlog_change",
        json!({"project_id":project.id,"backlog_id":planning,"request_id":"mcp-backlog-change","change":{"expected_revision":1,"name":"Planning revised","reason":"Rename planning queue"}}),
    );
    assert_eq!(
        changed_backlog["result"]["structuredContent"]["revision"],
        2
    );
    let (_, read_backlog) = rpc(
        "nexora_backlog_get",
        json!({"project_id":project.id,"backlog_id":planning}),
    );
    assert_eq!(
        read_backlog["result"]["structuredContent"]["name"],
        "Planning revised"
    );
    let second_input = json!({"title":"Another bug","type_code":"bug","backlog_id":backlog["id"],"origin":"External developer"});
    let (_, second) = rpc(
        "nexora_task_create",
        json!({"project_id":project.id,"request_id":"mcp-task-create","definition":second_input}),
    );
    assert_eq!(second["result"]["isError"], false);
    let (status, second_replay) = request(
        "POST",
        &format!("{base}/tasks"),
        &key.key,
        Some(second_input),
        Some("mcp-task-create"),
    );
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        second_replay["id"],
        second["result"]["structuredContent"]["id"]
    );
    let (status, current_backlog) = request(
        "GET",
        &format!("{base}/backlogs/{}", backlog["id"].as_str().unwrap()),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    let (_, ordered) = rpc(
        "nexora_backlog_reorder",
        json!({"project_id":project.id,"backlog_id":backlog["id"],"request_id":"mcp-reorder","order":{"expected_revision":current_backlog["revision"],"task_id":second_replay["id"],"before_id":id}}),
    );
    assert_eq!(ordered["result"]["isError"], false);
    let (status, ordered_page) = request(
        "GET",
        &format!(
            "{base}/tasks?backlog_id={}",
            backlog["id"].as_str().unwrap()
        ),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ordered_page["items"][0]["id"], second_replay["id"]);
    let (status, history) = request(
        "GET",
        &format!("{base}/tasks/{id}/history?limit=1"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
    assert!(!history["next_after"].is_null());
    store.revoke_key(&ctx, &key.metadata.id).unwrap();
    let (status, _) = request("GET", &format!("{base}/tasks/{id}"), &key.key, None, None);
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
