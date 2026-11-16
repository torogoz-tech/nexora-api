#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::{Value, json};
use tower::ServiceExt;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; public REST/MCP subtask and prerequisite execution rules"]
fn task_relations_enforce_prerequisites_and_child_completion_over_http_and_mcp() {
    let mut f = WorkItemFixture::new();
    let password = zeroize::Zeroizing::new(
        nexora_platform_store::SecretGenerator::token("pool_http").unwrap(),
    );
    let actor = f
        .store
        .create_user(&f.ctx, "pool_http_developer", &password, "Pool developer")
        .unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &actor.id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: true,
                project_ids: vec![],
                expected_revision: 0,
            },
        )
        .unwrap();
    let session = f.store.login("pool_http_developer", &password).unwrap();
    let owner_session = f.store.effective_permissions(&f.ctx).unwrap();
    assert_eq!(owner_session["role"], "super_admin");
    let flow_input = serde_json::from_value(json!({"name":"Pool manufacturing","graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"","from":"ready","to":"done","guards":[]}]}})).unwrap();
    let flow = f
        .store
        .create_developer_workflow(&f.ctx, &flow_input)
        .unwrap();
    let version = f
        .store
        .publish_developer_workflow(
            &f.ctx,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "HTTP fixture".into(),
            },
        )
        .unwrap();
    f.store
        .assign_actor_workflow(
            &f.ctx,
            &actor.id,
            &DeveloperWorkflowChange {
                expected_revision: 0,
                workflow_version_id: version.id.clone(),
                reason: "HTTP fixture".into(),
            },
        )
        .unwrap();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            &actor.id,
            "Pool read agent",
            KeyScope {
                capabilities: vec![Capability::TasksRead],
                project_ids: vec![],
                all_projects: false,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let app = runtime.block_on(async {
        ApiServer::router(
            ApiState::new(f.config.clone(), f.namespace.clone(), 4)
                .await
                .unwrap(),
        )
    });
    let request =
        |method: &str, path: &str, token: &str, body: Option<Value>, receipt: Option<&str>| {
            runtime.block_on(async {
                let mut builder = Request::builder()
                    .method(method)
                    .uri(path)
                    .header("authorization", format!("Bearer {token}"))
                    .header("accept", "application/json, text/event-stream");
                if body.is_some() {
                    builder = builder.header("content-type", "application/json");
                }
                if let Some(id) = receipt {
                    builder = builder.header("idempotency-key", id);
                }
                let response = app
                    .clone()
                    .oneshot(
                        builder
                            .body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let status = response.status();
                let bytes = response.into_body().collect().await.unwrap().to_bytes();
                (status, serde_json::from_slice::<Value>(&bytes).unwrap())
            })
        };
    let token = session.access_token.as_str();
    let create = |title: &str, receipt: &str| {
        let (status, task) = request(
            "POST",
            "/api/v1/tasks",
            token,
            Some(json!({"title":title,"type_code":"task","context":"","origin":"web","fields":{}})),
            Some(receipt),
        );
        assert!(status.is_success(), "{status}: {task}");
        task
    };
    let parent = create("Parent manufacturing task", "relations-parent");
    let parent_id = parent["id"].as_str().unwrap();
    let parent_path = format!("/api/v1/tasks/{parent_id}");
    let (status, child) = request(
        "POST",
        &format!("{parent_path}/subtasks"),
        token,
        Some(
            json!({"expected_revision":parent["revision"],"definition":{"title":"First child work","type_code":"task","context":"","origin":"web","fields":{}}}),
        ),
        Some("relations-child"),
    );
    assert!(status.is_success(), "{status}: {child}");
    assert_eq!(child["lifecycle"], "pending");
    assert!(child["project_id"].is_null() && child["assignee_id"].is_null());
    let child_id = child["id"].as_str().unwrap();
    let child_path = format!("/api/v1/tasks/{child_id}");
    let blocker = create("Prerequisite work", "relations-blocker");
    let blocker_id = blocker["id"].as_str().unwrap();
    let blocker_path = format!("/api/v1/tasks/{blocker_id}");
    let refreshed = request("GET", &parent_path, token, None, None).1;
    let (status, dependency) = request(
        "POST",
        &format!("{parent_path}/dependencies"),
        token,
        Some(
            json!({"source_task_id":parent_id,"target_task_id":blocker_id,"kind":"depends_on","source_revision":refreshed["revision"],"target_revision":blocker["revision"],"reason":"Finish prerequisite before parent starts"}),
        ),
        Some("relations-dependency"),
    );
    assert!(status.is_success(), "{status}: {dependency}");
    let relations = request(
        "GET",
        &format!("{parent_path}/relations"),
        token,
        None,
        None,
    )
    .1;
    assert_eq!(relations["subtasks_total"], 1);
    assert_eq!(relations["unfinished_subtasks"], 1);
    assert_eq!(relations["blocking_count"], 1);
    assert_eq!(relations["subtasks"][0]["id"], child_id);
    let refreshed = request("GET", &parent_path, token, None, None).1;
    let (status, _) = request(
        "POST",
        &format!("{parent_path}/take"),
        token,
        Some(json!({"expected_revision":refreshed["revision"]})),
        Some("relations-blocked-take"),
    );
    assert!(!status.is_success(), "Blocked task must not start");
    assert_eq!(
        request("GET", &parent_path, token, None, None).1["lifecycle"],
        "pending"
    );
    let blocker = request("GET", &blocker_path, token, None, None).1;
    let (status, active) = request(
        "POST",
        &format!("{blocker_path}/take"),
        token,
        Some(json!({"expected_revision":blocker["revision"]})),
        Some("relations-take-blocker"),
    );
    assert!(status.is_success(), "{status}: {active}");
    let (status, completed) = request(
        "POST",
        &format!("{blocker_path}/transitions/finish"),
        token,
        Some(json!({"expected_revision":active["revision"]})),
        Some("relations-finish-blocker"),
    );
    assert!(status.is_success(), "{status}: {completed}");
    assert_eq!(
        request(
            "GET",
            &format!("{parent_path}/relations"),
            token,
            None,
            None
        )
        .1["blocking_count"],
        0
    );
    let parent = request("GET", &parent_path, token, None, None).1;
    let (status, active_parent) = request(
        "POST",
        &format!("{parent_path}/take"),
        token,
        Some(json!({"expected_revision":parent["revision"]})),
        Some("relations-take-parent"),
    );
    assert!(
        status.is_success(),
        "Parent may start with pending child: {status}: {active_parent}"
    );
    let (status, _) = request(
        "POST",
        &format!("{parent_path}/transitions/finish"),
        token,
        Some(json!({"expected_revision":active_parent["revision"]})),
        Some("relations-premature-parent-finish"),
    );
    assert!(!status.is_success(), "Parent must not finish before child");
    assert_eq!(
        request("GET", &parent_path, token, None, None).1["lifecycle"],
        "active"
    );
    let child = request("GET", &child_path, token, None, None).1;
    let (status, active_child) = request(
        "POST",
        &format!("{child_path}/take"),
        token,
        Some(json!({"expected_revision":child["revision"]})),
        Some("relations-take-child"),
    );
    assert!(status.is_success(), "{status}: {active_child}");
    let (status, completed_child) = request(
        "POST",
        &format!("{child_path}/transitions/finish"),
        token,
        Some(json!({"expected_revision":active_child["revision"]})),
        Some("relations-finish-child"),
    );
    assert!(status.is_success(), "{status}: {completed_child}");
    let parent = request("GET", &parent_path, token, None, None).1;
    let (status, completed_parent) = request(
        "POST",
        &format!("{parent_path}/transitions/finish"),
        token,
        Some(json!({"expected_revision":parent["revision"]})),
        Some("relations-finish-parent"),
    );
    assert!(status.is_success(), "{status}: {completed_parent}");
    assert_eq!(completed_parent["lifecycle"], "completed");
    let (status, mcp) = request(
        "POST",
        "/mcp",
        &key.key,
        Some(
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nexora_task_relations_get","arguments":{"task_id":parent_id}}}),
        ),
        None,
    );
    assert_eq!(status, StatusCode::OK, "{mcp}");
    assert_eq!(mcp["result"]["isError"], false, "{mcp}");
    assert!(mcp.to_string().contains(child_id));
}
