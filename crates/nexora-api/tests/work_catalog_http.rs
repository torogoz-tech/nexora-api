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
#[ignore = "requires PostgreSQL; bounded task type/backlog catalogs and idempotent changes"]
fn work_catalog_changes_and_metadata_pages_share_http_and_mcp() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_catalog_http_{}",
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
    let workflow=store.create_workflow(&ctx,&project.id,&serde_json::from_value(json!({"name":"Catalog flow","purpose":"Existing configured work policy","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"reason"}]}]}})).unwrap()).unwrap();
    let flow = store
        .publish_workflow(
            &ctx,
            &project.id,
            &workflow.id,
            &serde_json::from_value(
                json!({"expected_revision":workflow.revision,"reason":"Publish catalog fixture"}),
            )
            .unwrap(),
        )
        .unwrap();
    let types = format!("/api/v1/projects/{}/task-types", project.id);
    let backlogs = format!("/api/v1/projects/{}/backlogs", project.id);
    let (status, bug) = request(
        "POST",
        &types,
        &key.key,
        Some(json!({"code":"bug","name":"Bug"})),
        Some("create-bug"),
    );
    assert_eq!(status, StatusCode::CREATED);
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
    let (status, page) = request("GET", &format!("{types}?limit=1"), &key.key, None, None);
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["items"][0]["code"], "bug");
    assert_eq!(page["next_after"], "bug");
    let (_, page) = rpc(
        "nexora_task_types_list",
        json!({"project_id":project.id,"after":"bug","limit":1}),
    );
    assert_eq!(
        page["result"]["structuredContent"]["items"][0]["code"],
        "task"
    );
    let change = json!({"expected_revision":bug["revision"],"name":"Retired defect","archived":true,"reason":"Retire classification"});
    let path = format!("{types}/bug/change");
    let (status, changed) = request(
        "POST",
        &path,
        &key.key,
        Some(change.clone()),
        Some("change-type-once"),
    );
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["archived"], true);
    let (_, replay) = rpc(
        "nexora_task_type_change",
        json!({"project_id":project.id,"type_code":"bug","request_id":"change-type-once","change":change}),
    );
    assert_eq!(replay["result"]["structuredContent"], changed);
    assert_eq!(
        request("GET", &types, &key.key, None, None).1["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        request(
            "GET",
            &format!("{types}?include_archived=true"),
            &key.key,
            None,
            None
        )
        .1["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        request("GET", &format!("{types}?limit=0"), &key.key, None, None).0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, backlog) = request(
        "POST",
        &backlogs,
        &key.key,
        Some(json!({"name":"Delivery backlog","workflow_version_id":flow.id})),
        Some("create-backlog"),
    );
    assert_eq!(status, StatusCode::CREATED);
    let (status, page) = request("GET", &backlogs, &key.key, None, None);
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["items"][0]["id"], backlog["id"]);
    let (_, page) = rpc(
        "nexora_backlogs_list",
        json!({"project_id":project.id,"limit":1}),
    );
    assert_eq!(
        page["result"]["structuredContent"]["items"][0]["id"],
        backlog["id"]
    );
    let (status, archived) = request(
        "PATCH",
        &format!("{backlogs}/{}", backlog["id"].as_str().unwrap()),
        &key.key,
        Some(
            json!({"expected_revision":backlog["revision"],"archived":true,"reason":"Retire empty backlog"}),
        ),
        Some("retire-backlog"),
    );
    assert_eq!(status, StatusCode::OK, "{archived}");
    assert_eq!(
        request("GET", &backlogs, &key.key, None, None).1["items"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let (_, page) = rpc(
        "nexora_backlogs_list",
        json!({"project_id":project.id,"include_archived":true}),
    );
    assert_eq!(
        page["result"]["structuredContent"]["items"][0]["archived"],
        true
    );
}
