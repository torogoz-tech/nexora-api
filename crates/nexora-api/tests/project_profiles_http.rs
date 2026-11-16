#[path = "../../nexora-platform-store/tests/support/feature_map_fixture.rs"]
mod feature_map_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use feature_map_fixture::FeatureMapFixture;
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::Value;
use tower::ServiceExt;

#[test]
#[ignore = "requires PostgreSQL; project profile HTTP authorization"]
fn project_profile_http_validates_year_and_live_project_scope() {
    let mut f = FeatureMapFixture::new();
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.developer.actor_id(),
            "Project profile HTTP",
            KeyScope {
                capabilities: MemberRole::Developer.capabilities(),
                project_ids: vec![f.work.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let other = f
        .work
        .store
        .create_project(&f.work.ctx, "Other profile", None)
        .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let app = runtime.block_on(async {
        ApiServer::router(
            ApiState::new(f.work.config.clone(), f.work.namespace.clone(), 4)
                .await
                .unwrap(),
        )
    });
    let request = |project: &str, suffix: &str| {
        runtime.block_on(async {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/api/v1/projects/{project}/profile{suffix}"))
                        .header("authorization", format!("Bearer {}", key.key))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (
                status,
                serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            )
        })
    };
    let (status, profile) = request(&f.work.project.id, "");
    assert_eq!(status, StatusCode::OK, "{profile}");
    assert_eq!(profile["project"]["id"], f.work.project.id);
    assert_eq!(profile["counts"]["requirements"], 1);
    assert_eq!(request(&other.id, "").0, StatusCode::FORBIDDEN);
    assert_eq!(
        request(&f.work.project.id, "?year=1999").0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        request(&f.work.project.id, "?year=invalid").0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(&f.work.project.id, "?year=2000").1["contributions"],
        serde_json::json!([])
    );
    f.sql
        .execute(
            "UPDATE api_keys SET revoked_at=clock_timestamp() WHERE id=$1",
            &[&key.metadata.id],
        )
        .unwrap();
    assert_eq!(request(&f.work.project.id, "").0, StatusCode::UNAUTHORIZED);
}
