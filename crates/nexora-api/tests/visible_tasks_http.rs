#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::Value;
use tower::ServiceExt;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; authenticated visible task/type REST schemas, pagination and invalid filters"]
fn visible_task_http_contracts_match_the_list_and_navigation() {
    let mut f = WorkItemFixture::new();
    let task = f.task("HTTP visible task");
    f.store
        .create_task_type(
            &f.ctx,
            &f.project.id,
            &TaskTypeCreate {
                purpose: nexora_platform::TaskPurpose::General,
                code: "bug".into(),
                name: "Bugs".into(),
                description: None,
                color: None,
            },
        )
        .unwrap();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Task reader",
            KeyScope {
                capabilities: vec![Capability::TasksRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let app = rt.block_on(async {
        ApiServer::router(
            ApiState::new(f.config.clone(), f.namespace.clone(), 2)
                .await
                .unwrap(),
        )
    });
    let request = |path: &str, token: Option<&str>| {
        rt.block_on(async {
            let mut builder = Request::builder().uri(path);
            if let Some(token) = token {
                builder = builder.header("authorization", format!("Bearer {token}"))
            }
            let response = app
                .clone()
                .oneshot(builder.body(Body::empty()).unwrap())
                .await
                .unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            (status, value)
        })
    };
    let (status, page) = request("/api/v1/me/tasks", Some(&key.key));
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 1);
    assert_eq!(page["participants"], serde_json::json!([]));
    assert_eq!(page["items"][0]["id"], task.id);
    assert_eq!(page["items"][0]["state_name"], "Ready");
    assert!(page["items"][0]["assignee_name"].is_null());
    let (status, types) = request("/api/v1/me/task-types?limit=1", Some(&key.key));
    assert_eq!(status, StatusCode::OK);
    assert_eq!(types["total"], 1);
    assert_eq!(types["items"][0]["code"], "bug");
    assert_eq!(types["items"][0]["total"], 0);
    assert_eq!(types["next_after"], "bug");
    let (_, next) = request("/api/v1/me/task-types?after=bug&limit=1", Some(&key.key));
    assert_eq!(next["items"][0]["code"], "task");
    assert_eq!(next["items"][0]["total"], 1);
    assert_eq!(
        request("/api/v1/me/tasks?limit=0", Some(&key.key)).0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        request("/api/v1/me/tasks?unexpected=true", Some(&key.key)).0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        request("/api/v1/me/tasks", None).0,
        StatusCode::UNAUTHORIZED
    );
}
