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
#[ignore = "requires PostgreSQL; entity colors over HTTP and MCP"]
fn entity_colors_roundtrip_through_http_and_mcp_with_replay_and_authorization() {
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
    let _project = store.create_project(&ctx, "Workflow HTTP", None).unwrap();
    let key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Workflow client",
            KeyScope {
                capabilities: vec![
                    Capability::ProjectsRead,
                    Capability::ProjectsWrite,
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
                project_ids: vec![],
                all_projects: true,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let legacy_definition = json!({"name":"Legacy HTTP project","description":null});
    let legacy_project = store
        .request(
            &ctx,
            "project.create",
            "pre-color-project-retry",
            &legacy_definition,
            |store| store.create_project(&ctx, "Legacy HTTP project", None),
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

    let (status, replayed) = request(
        "POST",
        "/api/v1/projects",
        &key.key,
        Some(json!({"name":"Legacy HTTP project"})),
        Some("pre-color-project-retry"),
    );
    assert_eq!(status, StatusCode::CREATED, "{replayed}");
    assert_eq!(replayed["id"], legacy_project.id);
    let call = |name: &str, args: Value| {
        let (status, value) = request(
            "POST",
            "/mcp",
            &key.key,
            Some(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}),
            ),
            None,
        );
        assert_eq!(status, StatusCode::OK);
        value
    };
    let created = call(
        "nexora_project_create",
        json!({"request_id":"create-colored-project","definition":{"name":"Colored through MCP","color":"#abc123"}}),
    );
    assert_eq!(created["result"]["isError"], false, "{created}");
    let project = &created["result"]["structuredContent"];
    assert_eq!(project["color"], "#ABC123");
    let id = project["id"].as_str().unwrap();
    let args = json!({"project_id":id,"request_id":"clear-project-color","change":{"expected_revision":1,"name":"Colored through MCP","archived":false,"color":null}});
    let cleared = call("nexora_project_change", args.clone());
    assert_eq!(cleared["result"]["isError"], false, "{cleared}");
    assert!(cleared["result"]["structuredContent"]["color"].is_null());
    assert_eq!(
        call("nexora_project_change", args)["result"],
        cleared["result"]
    );

    let kind = call(
        "nexora_task_type_create",
        json!({"project_id":id,"request_id":"colored-type","definition":{"code":"bug","name":"Bug","color":"#ab12cd"}}),
    );
    assert_eq!(
        kind["result"]["structuredContent"]["color"], "#AB12CD",
        "{kind}"
    );
    let workflow = call(
        "nexora_workflow_create",
        json!({"project_id":id,"request_id":"colored-workflow","definition":{"name":"Visual flow","purpose":"State display","resource_kinds":["task"],"graph":{"states":[{"id":"todo","name":"Todo","initial":true,"color":"#aa22bb"}],"transitions":[{"id":"stay","name":"Stay","from":"todo","to":"todo"}]}}}),
    );
    assert_eq!(workflow["result"]["isError"], false, "{workflow}");
    let workflow_id = workflow["result"]["structuredContent"]["id"]
        .as_str()
        .unwrap();
    let version = call(
        "nexora_workflow_publish",
        json!({"project_id":id,"workflow_id":workflow_id,"request_id":"publish-colored-workflow","publish":{"expected_revision":1,"reason":"Display metadata"}}),
    );
    assert_eq!(
        version["result"]["structuredContent"]["graph"]["states"][0]["color"], "#AA22BB",
        "{version}"
    );
    let version_id = version["result"]["structuredContent"]["id"]
        .as_str()
        .unwrap();
    let backlog = call(
        "nexora_backlog_create",
        json!({"project_id":id,"request_id":"colored-backlog","definition":{"name":"Delivery","workflow_version_id":version_id,"color":"#88aaee"}}),
    );
    assert_eq!(
        backlog["result"]["structuredContent"]["color"], "#88AAEE",
        "{backlog}"
    );
    let backlog_id = backlog["result"]["structuredContent"]["id"]
        .as_str()
        .unwrap();
    let (_, backlog_http) = request(
        "GET",
        &format!("/api/v1/projects/{id}/backlogs/{backlog_id}"),
        &key.key,
        None,
        None,
    );
    assert_eq!(backlog_http["color"], "#88AAEE");
    let clear_args = json!({"project_id":id,"backlog_id":backlog_id,"request_id":"clear-backlog-color","change":{"expected_revision":1,"reason":"Default display","color":null}});
    let cleared_backlog = call("nexora_backlog_change", clear_args.clone());
    assert_eq!(
        cleared_backlog["result"]["isError"], false,
        "{cleared_backlog}"
    );
    assert_eq!(
        cleared_backlog["result"],
        call("nexora_backlog_change", clear_args)["result"]
    );
    assert!(cleared_backlog["result"]["structuredContent"]["color"].is_null());
    let (_, types_http) = request(
        "GET",
        &format!("/api/v1/projects/{id}/task-types"),
        &key.key,
        None,
        None,
    );
    assert_eq!(types_http["items"][0]["color"], "#AB12CD");
    let cleared_kind = call(
        "nexora_task_type_change",
        json!({"project_id":id,"type_code":"bug","request_id":"clear-type-color","change":{"expected_revision":1,"reason":"Default display","color":null}}),
    );
    assert_eq!(cleared_kind["result"]["isError"], false, "{cleared_kind}");
    assert!(cleared_kind["result"]["structuredContent"]["color"].is_null());
    let (status, value) = request(
        "GET",
        &format!("/api/v1/projects/{id}"),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert!(value["color"].is_null());
    let (status, value) = request(
        "POST",
        "/api/v1/projects",
        &key.key,
        Some(json!({"name":"HTTP color","color":"#123abc"})),
        Some("http-colored-project"),
    );
    assert_eq!(status, StatusCode::CREATED, "{value}");
    assert_eq!(value["color"], "#123ABC");
    let (status, _) = request(
        "POST",
        "/api/v1/projects",
        &key.key,
        Some(json!({"name":"Invalid","color":"red"})),
        None,
    );
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let read_key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Read colors",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![],
                all_projects: true,
                installation: false,
            },
            600,
        )
        .unwrap();
    let (_, denied) = request(
        "POST",
        "/mcp",
        &read_key.key,
        Some(
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nexora_project_change","arguments":{"project_id":id,"request_id":"forbidden-color","change":{"expected_revision":2,"name":"Forbidden","archived":false,"color":"#FFFFFF"}}}}),
        ),
        None,
    );
    assert_eq!(denied["result"]["isError"], true, "{denied}");
    let (_, listed) = request(
        "POST",
        "/mcp",
        &key.key,
        Some(json!({"jsonrpc":"2.0","id":3,"method":"tools/list"})),
        None,
    );
    let tools = listed["result"]["tools"].as_array().unwrap();
    for (tool, definition) in [
        ("nexora_task_type_create", "TaskTypeCreate"),
        ("nexora_backlog_create", "BacklogCreate"),
    ] {
        let entry = tools.iter().find(|t| t["name"] == tool).unwrap();
        assert_eq!(
            entry["inputSchema"]["$defs"][definition]["properties"]["color"]["anyOf"][0]["pattern"],
            "^#[0-9A-Fa-f]{6}$"
        );
    }
}
