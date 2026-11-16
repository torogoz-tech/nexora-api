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
#[ignore = "requires PostgreSQL; global catalog/project selection REST and exact retries"]
fn catalog_and_project_selection_rest_roundtrip() {
    let mut f = WorkItemFixture::new();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Catalog admin",
            KeyScope {
                capabilities: Capability::all(),
                project_ids: vec![],
                all_projects: true,
                installation: true,
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
    let call = |method: &str, path: &str, body: Value, key_id: Option<&str>| {
        rt.block_on(async {
            let mut request = Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {}", key.key))
                .header("content-type", "application/json");
            if let Some(id) = key_id {
                request = request.header("Idempotency-Key", id)
            }
            let response = app
                .clone()
                .oneshot(
                    request
                        .body(if method == "GET" {
                            Body::empty()
                        } else {
                            Body::from(serde_json::to_vec(&body).unwrap())
                        })
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let body = response.into_body().collect().await.unwrap().to_bytes();
            (status, serde_json::from_slice::<Value>(&body).unwrap())
        })
    };
    let input = json!({"code":"custom_bug","name":"Defecto","icon":"flask-conical"});
    let (status, created) = call(
        "POST",
        "/api/v1/task-types",
        input.clone(),
        Some("create-bug"),
    );
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["name"], "Defecto");
    assert_eq!(created["icon"], "flask-conical");
    assert_eq!(
        call("POST", "/api/v1/task-types", input, Some("create-bug")).1,
        created
    );
    let path = format!("/api/v1/projects/{}/task-type-selection", f.project.id);
    let (status, selection) = call("GET", &path, Value::Null, None);
    assert_eq!(status, StatusCode::OK);
    let change = json!({"codes":["custom_bug"],"expected_revision":selection["revision"]});
    let (status, saved) = call("PUT", &path, change.clone(), Some("select-bug"));
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["codes"], json!(["custom_bug"]));
    assert_eq!(
        call("PUT", &path, change.clone(), Some("select-bug")).1,
        saved
    );
    assert_eq!(
        call("PUT", &path, change, Some("stale-select")).0,
        StatusCode::CONFLICT
    );
    let active = format!("/api/v1/projects/{}/task-types", f.project.id);
    let (status, types) = call("GET", &active, Value::Null, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(types["items"].as_array().unwrap().len(), 1);
    assert_eq!(types["items"][0]["code"], "custom_bug");
}
