use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use nexora_api::ApiCors;
use tower::ServiceExt;
#[tokio::test]
async fn cors_allows_only_the_configured_frontend_and_preflight_does_not_require_a_bearer() {
    let app = Router::new()
        .route("/protected", get(|| async { StatusCode::UNAUTHORIZED }))
        .layer(ApiCors::layer(&["https://nexora.infinitechsv.org".into()]).unwrap());
    let preflight = Request::builder()
        .method("OPTIONS")
        .uri("/protected")
        .header("origin", "https://nexora.infinitechsv.org")
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "authorization,content-type,idempotency-key",
        )
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(preflight).await.unwrap();
    assert!(response.status().is_success());
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "https://nexora.infinitechsv.org"
    );
    assert!(
        response
            .headers()
            .get("access-control-allow-credentials")
            .is_none()
    );
    let protected = Request::builder()
        .uri("/protected")
        .header("origin", "https://nexora.infinitechsv.org")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(protected).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "https://nexora.infinitechsv.org"
    );
    for origin in [
        "https://evil.invalid",
        "https://nexora.infinitechsv.org.evil.invalid",
        "http://nexora.infinitechsv.org",
        "null",
    ] {
        let request = Request::builder()
            .method("OPTIONS")
            .uri("/protected")
            .header("origin", origin)
            .header("access-control-request-method", "POST")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert!(
            response
                .headers()
                .get("access-control-allow-origin")
                .is_none(),
            "{origin}"
        );
    }
    for origin in [
        "*",
        "https://nexora.infinitechsv.org/path",
        "https://user:pass@example.invalid",
        "https://nexora.infinitechsv.org/",
    ] {
        assert!(ApiCors::layer(&[origin.into()]).is_err());
    }
}

#[tokio::test]
async fn cors_accepts_explicit_localhost_without_allowing_other_local_origins() {
    let app = Router::new()
        .route("/protected", get(|| async { StatusCode::UNAUTHORIZED }))
        .layer(ApiCors::layer(&["http://localhost:5173".into()]).unwrap());
    for (origin, allowed) in [
        ("http://localhost:5173", true),
        ("http://localhost:5174", false),
        ("http://localhost.evil.invalid:5173", false),
        ("http://127.0.0.1:5173", false),
    ] {
        let request = Request::builder()
            .method("OPTIONS")
            .uri("/protected")
            .header("origin", origin)
            .header("access-control-request-method", "GET")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .is_some(),
            allowed,
            "{origin}"
        );
    }
    for invalid in [
        "http://localhost:5173/",
        "http://localhost:5173/path",
        "http://localhost:5173?query=value",
        "http://user@localhost:5173",
        "http://localhost.evil.invalid:5173",
    ] {
        assert!(ApiCors::layer(&[invalid.into()]).is_err(), "{invalid}");
    }
}
