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
#[ignore = "requires PostgreSQL; public locale, installation-managed changes, optimistic locking and public SSE"]
fn global_language_roundtrips_and_reaches_anonymous_clients() {
    let mut f = WorkItemFixture::new();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Preferences admin",
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
            "Limited preferences",
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
    let call = |method: &str, path: &str, token: Option<&str>, input: Value, id: Option<&str>| {
        rt.block_on(async {
            let mut request = Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json");
            if let Some(token) = token {
                request = request.header("authorization", format!("Bearer {token}"))
            }
            if let Some(id) = id {
                request = request.header("Idempotency-Key", id)
            }
            let response = app
                .clone()
                .oneshot(
                    request
                        .body(if method == "GET" {
                            Body::empty()
                        } else {
                            Body::from(serde_json::to_vec(&input).unwrap())
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
    let (status, initial) = call("GET", "/api/v1/public/preferences", None, Value::Null, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(initial, json!({"language":"es","revision":1}));
    assert_eq!(
        call("GET", "/api/v1/settings/general", None, Value::Null, None).0,
        StatusCode::UNAUTHORIZED
    );
    let response = rt
        .block_on(
            app.clone().oneshot(
                Request::builder()
                    .uri("/api/v1/public/preferences/events")
                    .body(Body::empty())
                    .unwrap(),
            ),
        )
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body();
    let first = rt
        .block_on(stream.frame())
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    assert!(String::from_utf8_lossy(&first).contains("\"language\":\"es\""));
    let input = json!({"language":"en","expected_revision":1});
    assert_eq!(
        call(
            "PUT",
            "/api/v1/settings/general",
            Some(&limited.key),
            input.clone(),
            Some("denied")
        )
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, saved) = call(
        "PUT",
        "/api/v1/settings/general",
        Some(&key.key),
        input.clone(),
        Some("change-language"),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved, json!({"language":"en","revision":2}));
    assert_eq!(
        call(
            "PUT",
            "/api/v1/settings/general",
            Some(&key.key),
            input.clone(),
            Some("change-language")
        )
        .1,
        saved
    );
    assert_eq!(
        call(
            "PUT",
            "/api/v1/settings/general",
            Some(&key.key),
            input,
            Some("stale-language")
        )
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            "PUT",
            "/api/v1/settings/general",
            Some(&key.key),
            json!({"language":"fr","expected_revision":2}),
            Some("invalid-language")
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let event = rt
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(8), stream.frame()).await
        })
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    assert!(String::from_utf8_lossy(&event).contains("\"language\":\"en\""));
    assert_eq!(
        call("GET", "/api/v1/public/preferences", None, Value::Null, None).1,
        saved
    );
    let mut other = nexora_platform_store::PlatformStore::connect(&f.config, &f.namespace).unwrap();
    assert_eq!(
        other.public_installation_preferences().unwrap().language,
        InstallationLanguage::En
    );
}
