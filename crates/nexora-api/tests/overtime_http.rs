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
#[ignore = "requires PostgreSQL; overtime and timezone HTTP contracts with live sessions"]
fn overtime_http_requires_real_tasks_and_permissioned_settings() {
    let mut f = WorkItemFixture::new();
    let password =
        zeroize::Zeroizing::new(nexora_platform_store::SecretGenerator::token("password").unwrap());
    let user = f
        .store
        .create_user(&f.ctx, "overtime_http", &password, "Overtime Developer")
        .unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &user.id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![f.project.id.clone()],
                expected_revision: 0,
            },
        )
        .unwrap();
    let session = f.store.login("overtime_http", &password).unwrap();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Overtime machine denial",
            KeyScope {
                capabilities: Capability::all(),
                project_ids: vec![],
                all_projects: true,
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
    let request = |method: &str, path: &str, token: &str, body: Option<Value>| {
        runtime.block_on(async {
            let mut builder = Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {token}"));
            if body.is_some() {
                builder = builder.header("content-type", "application/json").header(
                    "idempotency-key",
                    format!("test-{method}-{path}").replace('/', "-"),
                );
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
    let (status, settings) = request("GET", "/api/v1/settings/timezone", token, None);
    assert_eq!(status, StatusCode::OK, "{settings}");
    assert_eq!(settings["timezone"], "UTC");
    assert_eq!(
        request("GET", "/api/v1/settings/timezone", &key.key, None).0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            "PATCH",
            "/api/v1/settings/timezone",
            token,
            Some(
                json!({"timezone":"America/El_Salvador","expected_revision":settings["revision"]})
            )
        )
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, listing) = request("GET", "/api/v1/overtime", token, None);
    assert_eq!(status, StatusCode::OK, "{listing}");
    assert_eq!(listing["can_request"], true);
    assert_eq!(listing["total_approved_minutes"], 0);
    assert_eq!(
        request("GET", "/api/v1/overtime", &key.key, None).0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request("GET", "/api/v1/overtime/evidence-candidates", token, None).0,
        StatusCode::OK
    );
    let missing_tasks = json!({"date":"2026-12-01","start_time":"18:00","end_time":"20:00","next_day":false,"reason":"Planned work","task_ids":[]});
    assert_eq!(
        request("POST", "/api/v1/overtime", token, Some(missing_tasks)).0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, calendar) = request("GET", "/api/v1/labor-calendar", token, None);
    assert_eq!(status, StatusCode::OK, "{calendar}");
    assert_eq!(calendar["settings"]["work_start"], "09:00");
    assert_eq!(calendar["settings"]["work_end"], "17:00");
    assert_eq!(calendar["settings"]["daily_minutes"], 480);
    assert_eq!(
        request(
            "GET",
            "/api/v1/overtime/missing/evidence-candidates",
            token,
            None
        )
        .0,
        StatusCode::NOT_FOUND
    );
}
