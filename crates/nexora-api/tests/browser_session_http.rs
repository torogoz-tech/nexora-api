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
#[ignore = "requires PostgreSQL; cookie login, TOTP gates, cross-tab access, CSRF and revocation"]
fn browser_session_cookies_share_access_without_exposing_bearers() {
    let mut f = WorkItemFixture::new();
    let password = nexora_platform_store::SecretGenerator::token("cookie_test").unwrap();
    let user = f
        .store
        .create_user(&f.ctx, "cookie_user", &password, "Cookie user")
        .unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &user.id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: true,
                project_ids: vec![],
                expected_revision: 0,
            },
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
    let call = |method: &str, path: &str, cookie: &str, browser: bool, body: Value| {
        runtime.block_on(async {
            let mut request = Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json");
            if !cookie.is_empty() {
                request = request.header("cookie", cookie);
            }
            if browser {
                request = request
                    .header("x-nexora-browser", "https://nexora.example")
                    .header("origin", "https://nexora.example");
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::from(body.to_string())).unwrap())
                .await
                .unwrap();
            let status = response.status();
            let cookies = response
                .headers()
                .get_all("set-cookie")
                .iter()
                .map(|v| v.to_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (
                status,
                cookies,
                serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            )
        })
    };
    let login = call(
        "POST",
        "/api/v1/auth/login",
        "",
        true,
        json!({"username":"cookie_user","password":password}),
    );
    assert_eq!(login.0, StatusCode::OK);
    assert!(login.2.get("access_token").is_none());
    assert_eq!(login.2["next_step"], "totp_enroll");
    assert!(login.1[0].starts_with("__Host-nexora_session="));
    assert!(login.1[0].contains("HttpOnly; SameSite=Lax; Path=/"));
    assert!(login.1[0].ends_with("; Secure"));
    let challenge = login.1[0].split(';').next().unwrap();
    assert_eq!(
        call("GET", "/api/v1/projects", challenge, true, Value::Null).0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            "POST",
            "/api/v1/auth/gate",
            challenge,
            false,
            json!({"action":"skip","code":""})
        )
        .0,
        StatusCode::FORBIDDEN
    );
    let ready = call(
        "POST",
        "/api/v1/auth/gate",
        challenge,
        true,
        json!({"action":"skip","code":""}),
    );
    assert_eq!(ready.0, StatusCode::OK);
    assert!(ready.2.get("access_token").is_none());
    assert_eq!(ready.2["stage"], "ready");
    let cookie = ready.1[0].split(';').next().unwrap();
    assert_ne!(cookie, challenge);
    // A second browser tab sends the same host cookie with no JavaScript bearer.
    let tab = call("GET", "/api/v1/auth/session", cookie, true, Value::Null);
    assert_eq!(tab.0, StatusCode::OK);
    assert_eq!(tab.2["actor_id"], user.id);
    assert!(tab.2["next_step"].is_null());
    assert!(tab.2.get("access_token").is_none());
    assert_eq!(
        call("GET", "/api/v1/projects", cookie, true, Value::Null).0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            "POST",
            "/api/v1/projects",
            cookie,
            false,
            json!({"name":"CSRF"})
        )
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            "GET",
            "/api/v1/projects",
            &format!("{cookie}; {cookie}"),
            true,
            Value::Null
        )
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call("POST", "/mcp", cookie, true, json!({})).0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call("DELETE", "/api/v1/auth/session", cookie, false, Value::Null).0,
        StatusCode::FORBIDDEN
    );
    let logout = call("DELETE", "/api/v1/auth/session", cookie, true, Value::Null);
    assert_eq!(logout.0, StatusCode::NO_CONTENT);
    assert!(logout.1[0].contains("Max-Age=0"));
    assert_eq!(
        call("GET", "/api/v1/auth/session", cookie, true, Value::Null).0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call("GET", "/api/v1/projects", cookie, true, Value::Null).0,
        StatusCode::UNAUTHORIZED
    );
}
