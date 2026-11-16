use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use tower::ServiceExt;
#[test]
#[ignore = "requires PostgreSQL; large model import is bounded per route and fingerprinted by digest"]
fn profile_import_accepts_valid_payload_larger_than_default_json_limit() {
    let config = nexora_runtime::DatabaseConnections::load(std::path::Path::new(
        "../../.nexora/local/connections.json",
    ))
    .unwrap();
    let namespace = nexora_platform_store::SecretGenerator::id("nexora_platform_profiles").unwrap();
    let mut store = nexora_platform_store::PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = zeroize::Zeroizing::new(
        nexora_platform_store::SecretGenerator::token("profile_http").unwrap(),
    );
    store
        .bootstrap("Profiles", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let mut package = nexora_language::AnalysisProfilePackage::from_catalog(
        std::path::Path::new("../../languages/lua"),
        "lua".into(),
        "large-transport".into(),
        "1.0.0".into(),
        "Large transport".into(),
    )
    .unwrap();
    package.models[0]
        .lexer
        .keywords
        .extend((0..1300).map(|i| format!("keyword{i}{}", "x".repeat(900))));
    let bytes = serde_json::to_vec(&package).unwrap();
    assert!(bytes.len() > 1_048_576);
    package.validate().unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let app = ApiServer::router(
            ApiState::new(config.clone(), namespace.clone(), 4)
                .await
                .unwrap(),
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/analysis-profiles")
                    .header("authorization", format!("Bearer {}", session.access_token))
                    .header("content-type", "application/json")
                    .header("idempotency-key", "profile-large-body")
                    .body(Body::from(bytes))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(
            status.is_success(),
            "{status}: {}",
            String::from_utf8_lossy(&body)
        );
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["key"], "large-transport");
    });
}
