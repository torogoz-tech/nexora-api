use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_language::LanguageDefinition;
use nexora_platform::*;
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::{Value, json};
use std::path::Path;
use tower::ServiceExt;
#[test]
#[ignore = "requires PostgreSQL; portable model import/export and current installation authority"]
fn analysis_profile_http_roundtrip_conflicts_and_revoked_authority() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = SecretGenerator::id("nexora_platform_profile_http").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password =
        zeroize::Zeroizing::new(SecretGenerator::token("profile_http_password").unwrap());
    store
        .bootstrap("Model portability test", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let project = store.create_project(&ctx, "Portable models", None).unwrap();
    let dev_password = zeroize::Zeroizing::new(SecretGenerator::token("profile_reader").unwrap());
    let member = store
        .create_user(&ctx, "profile_reader", &dev_password, "Profile reader")
        .unwrap();
    store
        .change_member_role(
            &ctx,
            &member.id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![project.id],
                expected_revision: 0,
            },
        )
        .unwrap();
    let developer = store.login("profile_reader", &dev_password).unwrap();
    let key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Admin key cannot install models",
            KeyScope {
                capabilities: Capability::all(),
                project_ids: vec![],
                all_projects: true,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let language = LanguageDefinition::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../languages/lua"),
    )
    .unwrap();
    let input = json!({"format":"nexora.analysis-profile","schema_version":1,"key":"http-lua","version":"1.0.0","name":"HTTP Lua model","language_id":"lua","models":[{"manifest":language.manifest,"lexer":language.lexer,"grammar":language.grammar,"graph":language.graph}]});
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let app = runtime.block_on(async {
        ApiServer::router(
            ApiState::new(config.clone(), namespace.clone(), 4)
                .await
                .unwrap(),
        )
    });
    let request =
        |method: &str, path: &str, token: &str, body: Option<Value>, receipt: Option<&str>| {
            runtime.block_on(async {
                let mut builder = Request::builder()
                    .method(method)
                    .uri(path)
                    .header("authorization", format!("Bearer {token}"));
                if body.is_some() {
                    builder = builder.header("content-type", "application/json");
                }
                if let Some(id) = receipt {
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
    let (status, created) = request(
        "POST",
        "/api/v1/analysis-profiles",
        &session.access_token,
        Some(input.clone()),
        Some("profile-http-import"),
    );
    assert!(status.is_success(), "{status}: {created}");
    assert_eq!(created["key"], "http-lua");
    assert_eq!(created["version"], "1.0.0");
    assert_eq!(
        request(
            "POST",
            "/api/v1/analysis-profiles",
            &session.access_token,
            Some(input.clone()),
            Some("profile-http-import")
        )
        .1,
        created
    );
    let id = created["id"].as_str().unwrap();
    let path = format!("/api/v1/analysis-profiles/{id}");
    let (status, detail) = request("GET", &path, &session.access_token, None, None);
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["checksum"], created["checksum"]);
    let (status, exported) = request(
        "GET",
        &format!("{path}/export"),
        &session.access_token,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK, "{exported}");
    assert_eq!(exported, input);
    let (status, reimported) = request(
        "POST",
        "/api/v1/analysis-profiles",
        &session.access_token,
        Some(exported),
        Some("profile-http-roundtrip"),
    );
    assert!(status.is_success(), "{status}: {reimported}");
    assert_eq!(reimported["id"], created["id"]);
    assert_eq!(reimported["checksum"], created["checksum"]);
    let mut changed = input.clone();
    changed["name"] = json!("Changed immutable model metadata");
    assert_eq!(
        request(
            "POST",
            "/api/v1/analysis-profiles",
            &session.access_token,
            Some(changed),
            Some("profile-http-version-conflict")
        )
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        request(
            "POST",
            "/api/v1/analysis-profiles",
            &developer.access_token,
            Some(input.clone()),
            Some("profile-http-developer-denied")
        )
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            "POST",
            "/api/v1/analysis-profiles",
            &key.key,
            Some(input.clone()),
            Some("profile-http-key-denied")
        )
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            "GET",
            &format!("{path}/export"),
            &developer.access_token,
            None,
            None
        )
        .0,
        StatusCode::FORBIDDEN
    );
    let mut invalid = input.clone();
    invalid["directory"] = json!("/etc/nexora");
    assert!(
        request(
            "POST",
            "/api/v1/analysis-profiles",
            &session.access_token,
            Some(invalid),
            Some("profile-http-no-paths")
        )
        .0
        .is_client_error()
    );
    store.logout(&session.access_token).unwrap();
    assert_eq!(
        request(
            "POST",
            "/api/v1/analysis-profiles",
            &session.access_token,
            Some(input),
            Some("profile-http-import")
        )
        .0,
        StatusCode::UNAUTHORIZED
    );
}
