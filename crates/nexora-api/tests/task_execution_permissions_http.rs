#[path = "../../nexora-platform-store/tests/support/task_pool_fixture.rs"]
mod task_pool_fixture;
#[path = "../../nexora-platform-store/tests/support/task_test_sql.rs"]
mod test_sql;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::{Value, json};
use tower::ServiceExt;

#[test]
#[ignore = "requires PostgreSQL"]
fn rest_and_mcp_share_execution_limits_and_manager_feature_completion() {
    let mut f = task_pool_fixture::TaskPoolFixture::new(false);
    let first = f.create(None, None);
    let second = f.create(None, None);
    let first = f
        .work
        .store
        .take_task(
            &f.developer,
            &first.id,
            &TaskTake {
                expected_revision: first.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    f.work
        .store
        .change_member_role(
            &f.work.ctx,
            f.developer.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: true,
                project_ids: vec![],
                expected_revision: 1,
            },
        )
        .unwrap();
    f.work
        .store
        .change_member_role(
            &f.work.ctx,
            f.manager.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Manager,
                all_projects: true,
                project_ids: vec![],
                expected_revision: 1,
            },
        )
        .unwrap();
    let scope = KeyScope {
        capabilities: vec![
            Capability::TasksRead,
            Capability::TasksWrite,
            Capability::TasksTransition,
            Capability::DocumentsRead,
            Capability::DocumentsWrite,
        ],
        project_ids: vec![],
        all_projects: true,
        installation: true,
    };
    let developer = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.developer.actor_id(),
            "Developer execution",
            KeyScope {
                capabilities: vec![
                    Capability::TasksRead,
                    Capability::TasksWrite,
                    Capability::TasksTransition,
                ],
                project_ids: vec![],
                all_projects: false,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let manager = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.manager.actor_id(),
            "Manager execution",
            scope,
            3600,
        )
        .unwrap();
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    sql.execute("INSERT INTO project_features(id,project_id,name,created_by) VALUES('http-complete',$1,'Complete through MCP',$2)",&[&f.work.project.id,&f.manager.actor_id()]).unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let app = runtime.block_on(async {
        ApiServer::router(
            ApiState::new(f.work.config.clone(), f.work.namespace.clone(), 4)
                .await
                .unwrap(),
        )
    });
    let request = |path: &str, token: &str, body: Value, key: &str| {
        runtime.block_on(async {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(path)
                        .header("authorization", format!("Bearer {token}"))
                        .header("content-type", "application/json")
                        .header("accept", "application/json, text/event-stream")
                        .header("idempotency-key", key)
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (status, serde_json::from_slice::<Value>(&bytes).unwrap())
        })
    };
    let (status, error) = request(
        &format!("/api/v1/tasks/{}/take", second.id),
        &developer.key,
        json!({"expected_revision":second.revision}),
        "rest-busy",
    );
    assert_eq!(status.as_u16(), 409, "{error}");
    assert!(
        error["title"]
            .as_str()
            .unwrap()
            .contains("pause the active task")
    );
    let mcp = |name: &str, arguments: Value| json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":arguments}});
    let (_, busy) = request(
        "/mcp",
        &developer.key,
        mcp(
            "nexora_task_take",
            json!({"task_id":second.id,"request_id":"mcp-busy","take":{"expected_revision":second.revision}}),
        ),
        "mcp-busy",
    );
    assert!(busy.to_string().contains("pause the active task"), "{busy}");
    let (_, pause) = request(
        "/mcp",
        &manager.key,
        mcp(
            "nexora_task_pause",
            json!({"task_id":first.id,"request_id":"mcp-manager-pause","expected_revision":first.revision}),
        ),
        "mcp-manager-pause",
    );
    assert!(!pause.to_string().contains("\"isError\":true"), "{pause}");
    assert_eq!(
        sql.query_one("SELECT lifecycle FROM tasks WHERE id=$1", &[&first.id])
            .unwrap()
            .get::<_, String>(0),
        "paused"
    );
    let path = format!(
        "/api/v1/projects/{}/features/http-complete/complete",
        f.work.project.id
    );
    let (status, error) = request(
        &path,
        &developer.key,
        json!({"expected_revision":1}),
        "dev-complete",
    );
    assert_eq!(status.as_u16(), 403, "{error}");
    let args = json!({"project_id":f.work.project.id,"feature_id":"http-complete","expected_revision":1,"request_id":"manager-complete"});
    let (_, result) = request(
        "/mcp",
        &manager.key,
        mcp("nexora_feature_complete", args.clone()),
        "manager-complete",
    );
    assert!(!result.to_string().contains("\"isError\":true"), "{result}");
    assert_eq!(
        sql.query_one(
            "SELECT status FROM project_features WHERE id='http-complete'",
            &[]
        )
        .unwrap()
        .get::<_, String>(0),
        "completed"
    );
    let (_, replayed) = request(
        "/mcp",
        &manager.key,
        mcp("nexora_feature_complete", args),
        "manager-complete",
    );
    assert_eq!(result["result"], replayed["result"]);
    assert_eq!(
        sql.query_one(
            "SELECT revision FROM project_features WHERE id='http-complete'",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        2
    );
}
