#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use tower::ServiceExt;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; authenticated SSE, committed changes, revocation, no project leakage"]
fn live_updates_authorize_recheck_and_invalidate() {
    let mut f = WorkItemFixture::new();
    let actor = f
        .store
        .create_actor(&f.ctx, ActorKind::Agent, "Live reader", "Project documents")
        .unwrap();
    f.store
        .grant(
            &f.ctx,
            &actor.id,
            Capability::DocumentsRead,
            Some(&f.project.id),
        )
        .unwrap();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            &actor.id,
            "Live reader",
            KeyScope {
                capabilities: vec![Capability::DocumentsRead],
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
    let call = |uri: String, token: Option<&str>| {
        let mut builder = Request::builder().uri(uri);
        if let Some(token) = token {
            builder = builder.header("authorization", format!("Bearer {token}"))
        };
        rt.block_on(app.clone().oneshot(builder.body(Body::empty()).unwrap()))
    };
    let uri = format!("/api/v1/events?project_id={}", f.project.id);
    assert_eq!(
        call(uri.clone(), None).unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call("/api/v1/events?project_id=hidden".into(), Some(&key.key))
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call("/api/v1/events".into(), Some(&key.key))
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = call(uri.clone(), Some(&key.key)).unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );
    assert_eq!(response.headers()["x-accel-buffering"], "no");
    assert!(
        response.headers()["cache-control"]
            .to_str()
            .unwrap()
            .contains("no-transform")
    );
    let mut body = response.into_body();
    let first = rt
        .block_on(body.frame())
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    let first = String::from_utf8_lossy(&first);
    assert!(first.contains("event: invalidate"));
    assert!(first.contains("documents"));
    assert!(!first.contains("knowledge"));
    let definition=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(serde_json::json!({"name":"Live documents","resource_kinds":["requirement"],"graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"published","name":"Published","publishable":true}],"transitions":[{"id":"publish","name":"Publish","from":"draft","to":"published","guards":[]}]}})).unwrap()).unwrap();
    let version = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &definition.id,
            &WorkflowPublish {
                expected_revision: definition.revision,
                reason: "Test".into(),
            },
        )
        .unwrap();
    f.store.create_project_document(&f.ctx,&f.project.id,&serde_json::from_value(serde_json::json!({"code":"LIVE-1","title":"Live requirement","content":{"kind":"requirement","body":{"problem":"Missing live refresh","objective":"Automatic refresh","scope":"Knowledge","business_actors":["Developer"],"acceptance_criteria":["New requirements appear"],"source":"User request"}},"origin":"declared_business","workflow_version_id":version.id,"reason":"Verify SSE"})).unwrap()).unwrap();
    let changed = rt
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(5), body.frame()).await
        })
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    assert!(String::from_utf8_lossy(&changed).contains("event: invalidate"));
    assert!(String::from_utf8_lossy(&changed).contains("documents"));
    assert!(!String::from_utf8_lossy(&changed).contains("Live requirement"));
    f.store
        .revoke_grant(
            &f.ctx,
            &actor.id,
            Capability::DocumentsRead,
            Some(&f.project.id),
        )
        .unwrap();
    let frame = rt
        .block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(5), body.frame()).await
        })
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    assert!(String::from_utf8_lossy(&frame).contains("event: access-lost"));
    assert!(rt.block_on(body.frame()).is_none());
}
