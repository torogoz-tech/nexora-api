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
#[ignore = "requires PostgreSQL; public REST/MCP task pool lifecycle"]
fn task_pool_http_separates_reservation_execution_and_completion() {
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
    let (status, created) = request(
        "POST",
        "/api/v1/tasks",
        token,
        Some(
            json!({"title":"Unassigned pool task","type_code":"task","context":"","origin":"web","fields":{}}),
        ),
        Some("pool-http-create"),
    );
    assert!(status.is_success(), "{status}: {created}");
    assert_eq!(created["lifecycle"], "pending");
    assert!(
        created["project_id"].is_null()
            && created["assignee_id"].is_null()
            && created["workflow_id"].is_null()
    );
    let (project_status, project_task) = request(
        "POST",
        "/api/v1/tasks",
        token,
        Some(
            json!({"project_id":f.project.id,"title":"Project task remains pending","type_code":"task","context":"","origin":"web","fields":{}}),
        ),
        Some("pool-http-project-create"),
    );
    assert!(
        project_status.is_success(),
        "{project_status}: {project_task}"
    );
    assert_eq!(project_task["project_id"], f.project.id);
    assert_eq!(project_task["lifecycle"], "pending");
    assert!(project_task["workflow_id"].is_null());
    let id = created["id"].as_str().unwrap();
    let path = format!("/api/v1/tasks/{id}");
    let (status, reserved) = request(
        "PATCH",
        &path,
        token,
        Some(
            json!({"expected_revision":created["revision"],"assignee_id":actor.id,"reason":"Responsibility reserved, work not started"}),
        ),
        Some("pool-http-reserve"),
    );
    assert_eq!(status, StatusCode::OK, "{reserved}");
    assert_eq!(reserved["lifecycle"], "pending");
    assert!(reserved["workflow_id"].is_null());
    let board = format!(
        "/api/v1/me/tasks?workflow_id={}&lifecycle=active&limit=100",
        flow.id
    );
    assert_eq!(request("GET", &board, token, None, None).1["total"], 0);
    let take_input = json!({"expected_revision":reserved["revision"]});
    let (status, taken) = request(
        "POST",
        &format!("{path}/take"),
        token,
        Some(take_input.clone()),
        Some("pool-http-take"),
    );
    assert_eq!(status, StatusCode::OK, "{taken}");
    assert_eq!(taken["lifecycle"], "active");
    assert_eq!(taken["workflow_version_id"], version.id);
    assert_eq!(request("GET", &board, token, None, None).1["total"], 1);
    assert_eq!(
        request(
            "POST",
            &format!("{path}/take"),
            token,
            Some(take_input),
            Some("pool-http-take")
        )
        .1,
        taken
    );
    let (status, completed) = request(
        "POST",
        &format!("{path}/transitions/finish"),
        token,
        Some(json!({"expected_revision":taken["revision"]})),
        Some("pool-http-finish"),
    );
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["lifecycle"], "completed");
    assert_eq!(request("GET", &board, token, None, None).1["total"], 0);
    let retained = request(
        "GET",
        "/api/v1/tasks?lifecycle=completed&limit=100",
        token,
        None,
        None,
    )
    .1;
    assert!(
        retained["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|task| task["id"] == id)
    );
    let (status, mcp) = request(
        "POST",
        "/mcp",
        &key.key,
        Some(
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nexora_task_get","arguments":{"task_id":id}}}),
        ),
        None,
    );
    assert_eq!(status, StatusCode::OK, "{mcp}");
    assert_eq!(mcp["result"]["isError"], false, "{mcp}");
    // Older negotiated transports may return text only; either representation must retain the task.
    assert!(mcp.to_string().contains(id));
}
