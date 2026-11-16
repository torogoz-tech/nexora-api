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
#[ignore = "requires PostgreSQL; real member profile events, live grants and API key attenuation"]
fn member_profiles_respect_identity_project_scope_and_real_activity() {
    let mut f = FeatureMapFixture::new();
    let password = zeroize::Zeroizing::new(
        nexora_platform_store::SecretGenerator::token("fixture_password").unwrap(),
    );
    let newcomer = f
        .work
        .store
        .create_user(&f.work.ctx, "newcomer", &password, "New member")
        .unwrap();
    let newcomer_session = f.work.store.login("newcomer", &password).unwrap();
    let owner_session = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Profile admin",
            KeyScope {
                capabilities: Capability::all(),
                project_ids: vec![],
                all_projects: true,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let developer_key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.developer.actor_id(),
            "Profile developer",
            KeyScope {
                capabilities: MemberRole::Developer.capabilities(),
                project_ids: vec![f.work.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let project_admin_key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Project scoped administration",
            KeyScope {
                capabilities: Capability::all(),
                project_ids: vec![f.work.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let document_key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.developer.actor_id(),
            "Document-only profile",
            KeyScope {
                capabilities: vec![Capability::DocumentsRead],
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
        .create_project(&f.work.ctx, "Private unrelated project", None)
        .unwrap();
    let unrelated_key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Other scope",
            KeyScope {
                capabilities: Capability::all(),
                project_ids: vec![other.id.clone()],
                all_projects: false,
                installation: true,
            },
            3600,
        )
        .unwrap();
    for index in 0..9 {
        f.sql.execute("INSERT INTO project_repositories(id,project_id,source_kind,local_name,authorization_status) VALUES($1,$2,'local_snapshot',$3,'authorized')", &[&format!("repo_profile_{index}"), &f.work.project.id, &format!("Registered local source {index}")]).unwrap();
    }
    f.sql.execute("INSERT INTO project_repositories(id,project_id,source_kind,local_name,authorization_status) VALUES('repo_profile_revoked',$1,'local_snapshot','Revoked source','revoked')", &[&f.work.project.id]).unwrap();
    let mut completed_task = String::new();
    for index in 0..18 {
        let task = f
            .work
            .store
            .create_task(
                &f.developer,
                &f.work.project.id,
                &TaskCreate {
                    title: format!("Priority contribution {index}"),
                    type_code: "task".into(),
                    assignee_id: Some(f.developer.actor_id().into()),
                    priority: TaskPriority::High,
                    origin: "Profile acceptance".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        if index == 17 {
            completed_task = task.id.clone();
            f.sql
                .execute("UPDATE tasks SET state_id='done' WHERE id=$1", &[&task.id])
                .unwrap();
        }
    }
    let executor = tokio::runtime::Runtime::new().unwrap();
    let app = executor.block_on(async {
        ApiServer::router(
            ApiState::new(f.work.config.clone(), f.work.namespace.clone(), 4)
                .await
                .unwrap(),
        )
    });
    let request = |member: &str, token: &str, suffix: &str| {
        executor.block_on(async {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/api/v1/members/{member}/profile{suffix}"))
                        .header("authorization", format!("Bearer {token}"))
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
    let (status, empty) = request(&newcomer.id, &newcomer_session.access_token, "");
    assert_eq!(status, StatusCode::OK, "{empty}");
    for key in [
        "projects",
        "repositories",
        "priority_tasks",
        "contributions",
        "activity",
    ] {
        assert_eq!(empty[key], serde_json::json!([]), "{key}");
    }
    assert_eq!(empty["member"]["username"], "newcomer");
    assert!(empty["member"].get("password_hash").is_none());
    assert_eq!(
        request(f.developer.actor_id(), &newcomer_session.access_token, "").0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&newcomer.id, &developer_key.key, "").0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(f.developer.actor_id(), &project_admin_key.key, "").0,
        StatusCode::FORBIDDEN
    );
    let (status, profile) = request(f.developer.actor_id(), &developer_key.key, "");
    assert_eq!(status, StatusCode::OK, "{profile}");
    assert_eq!(profile["priority_tasks"].as_array().unwrap().len(), 8);
    assert_eq!(profile["activity"].as_array().unwrap().len(), 15);
    assert!(
        profile["priority_tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|task| task["id"] != completed_task)
    );
    assert_eq!(profile["repositories"].as_array().unwrap().len(), 6);
    for repository in profile["repositories"].as_array().unwrap() {
        assert_eq!(repository["status"], "authorized");
        assert!(repository["provider"].is_null());
        assert!(
            repository["name"]
                .as_str()
                .unwrap()
                .starts_with("Registered local source")
        );
    }
    let count: i64 = f.sql.query_one("SELECT (SELECT count(*) FROM task_history WHERE actor_id=$1)+(SELECT count(*) FROM project_document_versions WHERE created_by=$1)", &[&f.developer.actor_id()]).unwrap().get(0);
    let chart_count: i64 = profile["contributions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|day| day["count"].as_i64().unwrap())
        .sum();
    assert_eq!(chart_count, count);
    assert_eq!(chart_count, 19);
    let (status, documents) = request(f.developer.actor_id(), &document_key.key, "");
    assert_eq!(status, StatusCode::OK, "{documents}");
    assert_eq!(documents["priority_tasks"], serde_json::json!([]));
    assert_eq!(documents["activity"].as_array().unwrap().len(), 1);
    assert_eq!(documents["activity"][0]["kind"], "document");
    let (status, attenuated) = request(f.developer.actor_id(), &unrelated_key.key, "");
    assert_eq!(status, StatusCode::OK, "{attenuated}");
    for key in [
        "projects",
        "repositories",
        "priority_tasks",
        "contributions",
        "activity",
    ] {
        assert_eq!(attenuated[key], serde_json::json!([]), "{key}");
    }
    assert_eq!(
        request(f.developer.actor_id(), &owner_session.key, "?year=1999").0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        request(f.developer.actor_id(), &owner_session.key, "?year=2000").1["contributions"],
        serde_json::json!([])
    );
    f.sql
        .execute(
            "UPDATE actors SET active=false WHERE id=$1",
            &[&newcomer.id],
        )
        .unwrap();
    assert_eq!(
        request(&newcomer.id, &newcomer_session.access_token, "").0,
        StatusCode::UNAUTHORIZED
    );
    // Removal takes effect for the existing credential, without reauthentication.
    f.sql
        .execute(
            "DELETE FROM actor_grants WHERE actor_id=$1",
            &[&f.developer.actor_id()],
        )
        .unwrap();
    let (status, revoked_grants) = request(f.developer.actor_id(), &developer_key.key, "");
    assert_eq!(status, StatusCode::OK, "{revoked_grants}");
    assert_eq!(revoked_grants["activity"], serde_json::json!([]));
    f.sql
        .execute(
            "UPDATE api_keys SET revoked_at=clock_timestamp() WHERE id=$1",
            &[&developer_key.metadata.id],
        )
        .unwrap();
    assert_eq!(
        request(f.developer.actor_id(), &developer_key.key, "").0,
        StatusCode::UNAUTHORIZED
    );
}
