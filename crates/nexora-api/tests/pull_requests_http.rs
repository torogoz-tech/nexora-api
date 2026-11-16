use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use nexora_api::{ApiServer, ApiState};
use nexora_platform::{Capability, KeyScope};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
#[test]
#[ignore = "requires PostgreSQL; isolated route and authorization checks"]
fn pr_routes_reject_missing_authority_and_invalid_inputs_before_provider_calls() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_pr_http_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = zeroize::Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    store
        .bootstrap("PR fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let project = store.create_project(&ctx, "PR routes", None).unwrap();
    let key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "No repository access",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let app = rt.block_on(async {
        ApiServer::router(
            ApiState::new(config.clone(), namespace.clone(), 2)
                .await
                .unwrap(),
        )
    });
    let base = format!(
        "/api/v1/projects/{}/repositories/repo_missing/pull-requests",
        project.id
    );
    for (path, token, expected) in [
        (base.clone(), "", StatusCode::UNAUTHORIZED),
        (
            format!("{base}/1/reviews"),
            key.key.as_str(),
            StatusCode::FORBIDDEN,
        ),
        (
            format!("{base}/1/reviews/missing"),
            key.key.as_str(),
            StatusCode::FORBIDDEN,
        ),
        (base.clone(), key.key.as_str(), StatusCode::FORBIDDEN),
        (
            format!("{base}/1/files"),
            key.key.as_str(),
            StatusCode::FORBIDDEN,
        ),
        (
            format!("{base}/1/links"),
            key.key.as_str(),
            StatusCode::FORBIDDEN,
        ),
        (
            format!("{base}?page=10001"),
            session.access_token.as_str(),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        let mut req = Request::builder().uri(path);
        if !token.is_empty() {
            req = req.header("authorization", format!("Bearer {token}"));
        }
        let response = rt
            .block_on(app.clone().oneshot(req.body(Body::empty()).unwrap()))
            .unwrap();
        assert_eq!(response.status(), expected);
    }
    drop(app);
    drop(store);
    drop(rt);
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("DROP SCHEMA \"{namespace}\" CASCADE"))
        .unwrap();
    std::fs::remove_file(
        config
            .secrets_directory
            .join(format!("{namespace}_master_key_v1")),
    )
    .unwrap();
}
