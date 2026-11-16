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
#[ignore = "requires PostgreSQL; email template HTTP contract and authorization; no external calls"]
fn email_contents_http_preserves_languages_and_validates_required_links() {
    let mut f = WorkItemFixture::new();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Settings fixture",
            KeyScope {
                capabilities: vec![Capability::InstallationManage],
                project_ids: vec![],
                all_projects: false,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let limited = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Read fixture",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
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
    let call = |method: &str, path: &str, token: Option<&str>, body: Value, id: Option<&str>| {
        rt.block_on(async {
            let mut request = Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json");
            if let Some(token) = token {
                request = request.header("authorization", format!("Bearer {token}"));
            }
            if let Some(id) = id {
                request = request.header("idempotency-key", id);
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
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (status, serde_json::from_slice::<Value>(&bytes).unwrap())
        })
    };
    let list = "/api/v1/settings/email-contents";
    assert_eq!(
        call("GET", list, None, Value::Null, None).0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call("GET", list, Some(&limited.key), Value::Null, None).0,
        StatusCode::FORBIDDEN
    );
    let (status, rows) = call("GET", list, Some(&key.key), Value::Null, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rows.as_array().unwrap().len(), 14);
    let path = "/api/v1/settings/email-contents/invitation/es";
    let (_, initial) = call("GET", path, Some(&key.key), Value::Null, None);
    let revision = initial["revision"].as_i64().unwrap();
    let missing = json!({"expected_revision":revision,"subject":"Invitation","body_html":"{{invitation_url}} {{expires_at}}"});
    assert_eq!(
        call("PUT", path, Some(&key.key), missing, Some("missing-link")).0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let body = json!({"expected_revision":revision,"subject":"Te invitamos","body_html":initial["body_html"]});
    let (status, saved) = call(
        "PUT",
        path,
        Some(&key.key),
        body.clone(),
        Some("save-email-content"),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["customized"], true);
    assert_eq!(
        call(
            "PUT",
            path,
            Some(&key.key),
            body.clone(),
            Some("save-email-content")
        )
        .1,
        saved
    );
    assert_eq!(
        call(
            "PUT",
            path,
            Some(&key.key),
            body,
            Some("stale-email-content")
        )
        .0,
        StatusCode::CONFLICT
    );
    let (_, english) = call(
        "GET",
        "/api/v1/settings/email-contents/invitation/en",
        Some(&key.key),
        Value::Null,
        None,
    );
    assert_eq!(english["customized"], false);
    assert_eq!(
        call(
            "GET",
            "/api/v1/settings/email-contents/invitation/fr",
            Some(&key.key),
            Value::Null,
            None
        )
        .0,
        StatusCode::NOT_FOUND
    );
}
