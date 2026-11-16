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
#[ignore = "requires PostgreSQL; calendar/rating HTTP sessions, idempotency and privacy"]
fn calendar_and_ratings_http_require_human_sessions_and_idempotent_mutations() {
    let mut f = WorkItemFixture::new();
    let password =
        zeroize::Zeroizing::new(nexora_platform_store::SecretGenerator::token("password").unwrap());
    let user = f
        .store
        .create_user(
            &f.ctx,
            "calendar_developer",
            &password,
            "Calendar Developer",
        )
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
    let session = f.store.login("calendar_developer", &password).unwrap();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Calendar key cannot impersonate human",
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
    let request =
        |method: &str, path: &str, token: &str, body: Option<Value>, idempotency: Option<&str>| {
            runtime.block_on(async {
                let mut builder = Request::builder()
                    .method(method)
                    .uri(path)
                    .header("authorization", format!("Bearer {token}"));
                if body.is_some() {
                    builder = builder.header("content-type", "application/json");
                }
                if let Some(id) = idempotency {
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
    let (status, calendar) = request("GET", "/api/v1/labor-calendar", token, None, None);
    assert_eq!(status, StatusCode::OK, "{calendar}");
    assert_eq!(calendar["can_request"], true);
    assert_eq!(calendar["can_manage"], false);
    assert_eq!(
        request("GET", "/api/v1/labor-calendar", &key.key, None, None).0,
        StatusCode::FORBIDDEN
    );
    let input = json!({"kind":"vacation","start_date":"2026-12-01","end_date":"2026-12-02","minutes_per_day":null,"reason":"Family vacation"});
    assert_eq!(
        request(
            "POST",
            "/api/v1/labor-calendar/requests",
            token,
            Some(input.clone()),
            None
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, created) = request(
        "POST",
        "/api/v1/labor-calendar/requests",
        token,
        Some(input.clone()),
        Some("calendar-http-request"),
    );
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(
        request(
            "POST",
            "/api/v1/labor-calendar/requests",
            token,
            Some(input),
            Some("calendar-http-request")
        )
        .1["id"],
        created["id"]
    );
    let decision_path = format!(
        "/api/v1/labor-calendar/requests/{}/decision",
        created["id"].as_str().unwrap()
    );
    assert_eq!(
        request(
            "POST",
            &decision_path,
            token,
            Some(json!({"approved":true,"reason":"self"})),
            Some("self-decision")
        )
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request("GET", "/api/v1/ratings", token, None, None).0,
        StatusCode::OK
    );
    let profile = format!("/api/v1/members/{}/ratings?year=2026", user.id);
    let (status, report) = request("GET", &profile, token, None, None);
    assert_eq!(status, StatusCode::OK, "{report}");
    assert_eq!(report["can_rate"], false);
    assert_eq!(
        request("GET", &profile, &key.key, None, None).0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(request("PUT", "/api/v1/settings/labor-calendar", token, Some(json!({"timezone":"UTC","country":null,"workdays":[1,2,3,4,5],"daily_minutes":480,"starts_on":"2026-11-07","expected_revision":1})), Some("unauthorized-calendar-change")).0, StatusCode::FORBIDDEN);
}
