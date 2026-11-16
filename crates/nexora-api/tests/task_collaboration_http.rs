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
fn task_dependencies_and_comments_use_the_same_scoped_services_over_http_and_mcp() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_collaboration_http_{}",
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
    let workflow=store.create_workflow(&ctx,&project.id,&serde_json::from_value(json!({"name":"Delivery","purpose":"Dependency gate","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"dependencies_satisfied"}]}]}})).unwrap()).unwrap();
    let version = store
        .publish_workflow(
            &ctx,
            &project.id,
            &workflow.id,
            &serde_json::from_value(
                json!({"expected_revision":workflow.revision,"reason":"Fixture flow"}),
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
    let input = json!({"title":"Upstream work","type_code":"task","workflow_version_id":version.id,"origin":"Fixture"});
    let a = store
        .create_task(
            &ctx,
            &project.id,
            &serde_json::from_value(input.clone()).unwrap(),
        )
        .unwrap();
    let b = store
        .create_task(&ctx, &project.id, &serde_json::from_value(input).unwrap())
        .unwrap();
    let base = format!("/api/v1/projects/{}", project.id);
    let link = json!({"source_task_id":a.id,"target_task_id":b.id,"kind":"blocks","source_revision":a.revision,"target_revision":b.revision,"reason":"Upstream condition"});
    let (status, dependency) = request(
        "POST",
        &format!("{base}/task-dependencies"),
        &key.key,
        Some(link),
        Some("create-link"),
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
    let (_, listed) = rpc(
        "nexora_task_dependencies_list",
        json!({"project_id":project.id,"task_id":b.id,"query":{"limit":10}}),
    );
    assert_eq!(
        listed["result"]["structuredContent"]["items"][0]["id"],
        dependency["id"]
    );
    let current = store.get_task(&ctx, &project.id, &b.id).unwrap();
    let (_, blocked) = rpc(
        "nexora_task_transition",
        json!({"project_id":project.id,"task_id":b.id,"transition_id":"finish","request_id":"blocked-finish","parameters":{"expected_revision":current.revision}}),
    );
    assert_eq!(blocked["result"]["isError"], true);
    let (_, resolved) = rpc(
        "nexora_task_dependency_resolve",
        json!({"project_id":project.id,"dependency_id":dependency["id"],"request_id":"resolve-link","change":{"expected_revision":dependency["revision"],"reason":"Condition verified"}}),
    );
    assert_eq!(
        resolved["result"]["structuredContent"]["status"],
        "resolved"
    );
    let current = store.get_task(&ctx, &project.id, &b.id).unwrap();
    let (status, finished) = request(
        "POST",
        &format!("{base}/tasks/{}/transitions/finish", b.id),
        &key.key,
        Some(json!({"expected_revision":current.revision})),
        Some("now-finish"),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(finished["state_id"], "done");
    let comment = json!({"body":"Business explanation with source provenance"});
    let (_, added) = rpc(
        "nexora_task_comment_add",
        json!({"project_id":project.id,"task_id":a.id,"request_id":"comment-once","comment":comment}),
    );
    assert_eq!(
        added["result"]["structuredContent"]["actor_id"],
        ctx.actor_id()
    );
    let (status, replay) = request(
        "POST",
        &format!("{base}/tasks/{}/comments", a.id),
        &key.key,
        Some(comment),
        Some("comment-once"),
    );
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(replay["id"], added["result"]["structuredContent"]["id"]);
    let (_, comments) = rpc(
        "nexora_task_comments_list",
        json!({"project_id":project.id,"task_id":a.id,"limit":10}),
    );
    assert_eq!(
        comments["result"]["structuredContent"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (status, comments) = request(
        "GET",
        &format!("{base}/tasks/{}/comments?limit=1", a.id),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(comments["items"][0]["id"], replay["id"]);
    let (status, removed) = request(
        "POST",
        &format!(
            "{base}/task-dependencies/{}/remove",
            dependency["id"].as_str().unwrap()
        ),
        &key.key,
        Some(
            json!({"expected_revision":resolved["result"]["structuredContent"]["revision"],"reason":"Retain historical relation"}),
        ),
        Some("remove-link"),
    );
    assert_eq!(status, StatusCode::OK);
    assert!(!removed["removed_at"].is_null());
    let (status, page) = request(
        "GET",
        &format!("{base}/tasks/{}/dependencies?include_removed=true", a.id),
        &key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["items"][0]["id"], dependency["id"]);
    let a = store.get_task(&ctx, &project.id, &a.id).unwrap();
    let b = store.get_task(&ctx, &project.id, &b.id).unwrap();
    let (_, related) = rpc(
        "nexora_task_dependency_create",
        json!({"project_id":project.id,"request_id":"related-link","dependency":{"source_task_id":a.id,"target_task_id":b.id,"source_revision":a.revision,"target_revision":b.revision,"kind":"related_to","reason":"Related context"}}),
    );
    assert_eq!(related["result"]["isError"], false);
    let (status, related_resolved) = request(
        "POST",
        &format!(
            "{base}/task-dependencies/{}/resolve",
            related["result"]["structuredContent"]["id"]
                .as_str()
                .unwrap()
        ),
        &key.key,
        Some(json!({"expected_revision":1,"reason":"Reference reviewed"})),
        Some("resolve-related"),
    );
    assert_eq!(status, StatusCode::OK);
    let (_, removed) = rpc(
        "nexora_task_dependency_remove",
        json!({"project_id":project.id,"dependency_id":related["result"]["structuredContent"]["id"],"request_id":"remove-related","change":{"expected_revision":related_resolved["revision"],"reason":"Remove context link"}}),
    );
    assert_eq!(removed["result"]["isError"], false);
    let (_, denied) = rpc(
        "nexora_task_comments_list",
        json!({"project_id":"foreign-project","task_id":a.id}),
    );
    assert_eq!(denied["result"]["isError"], true);
}
