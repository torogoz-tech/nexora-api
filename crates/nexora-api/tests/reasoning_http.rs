#[path = "../../nexora-platform-store/tests/support/local_snapshot_fixture.rs"]
mod local_snapshot_fixture;
#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use local_snapshot_fixture::LocalSnapshotFixture;
use nexora_api::{ApiServer, ApiState, ReasoningSettings};
use nexora_platform::{Capability, KeyScope};
use serde_json::{Value, json};
use tower::ServiceExt;
#[test]
#[ignore = "requires local databases; no external OpenAI requests"]
fn reasoning_boundary_checks_scope_before_accessing_the_provider() {
    let mut f = LocalSnapshotFixture::new();
    let published = f.adopt();
    let project = f.work.project.id.clone();
    let repository = published.repository_id.clone();
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Reasoning fixture",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead, Capability::KnowledgeRead],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let app = runtime.block_on(async {
        ApiServer::router(
            ApiState::new(f.work.config.clone(), f.work.namespace.clone(), 2)
                .await
                .unwrap()
                .with_reasoning(Some(ReasoningSettings {
                    key_file: std::path::PathBuf::from("/nonexistent/nexora-reasoning-test-key"),
                    model: "fixture-model".into(),
                    effort: "medium".into(),
                })),
        )
    });
    let call = |path: String, token: Option<&str>, body: Value| {
        runtime.block_on(async {
            let mut req = Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .header("mcp-protocol-version", "2025-11-25");
            if let Some(token) = token {
                req = req.header("authorization", format!("Bearer {token}"));
            }
            let response = app
                .clone()
                .oneshot(
                    req.body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (status, serde_json::from_slice::<Value>(&bytes).unwrap())
        })
    };
    let original = f
        .runtime
        .query_header(
            &f.header.tenant_id,
            &f.header.repository_id,
            Some(&f.header.id),
        )
        .unwrap();
    let receipts_before: i64 = f
        .sql
        .query_one("SELECT count(*) FROM request_receipts", &[])
        .unwrap()
        .get(0);
    let body = json!({"analysis":{"purpose":"code_review","objective":"Check guard","criteria":[{"id":"C1","description":"Guard exists"}]}});
    let path = format!("/api/v1/projects/{project}/repositories/{repository}/analyses");
    assert_eq!(
        call(path.clone(), None, body.clone()).0,
        StatusCode::UNAUTHORIZED
    );
    let other = format!("/api/v1/projects/other/repositories/{repository}/analyses");
    let denied = call(other, Some(&key.key), body.clone()).0;
    assert!(matches!(
        denied,
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ));
    // Authorized source, but absent provider credential: explicit unavailable, no inference.
    assert_eq!(
        call(path.clone(), Some(&key.key), body.clone()).0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let mut missing_baseline = body.clone();
    missing_baseline["baseline_snapshot"] = json!("not-published");
    assert_eq!(
        call(path.clone(), Some(&key.key), missing_baseline).0,
        StatusCode::NOT_FOUND
    );
    let mut same_baseline = body.clone();
    same_baseline["baseline_snapshot"] = json!(f.header.id.0);
    assert_eq!(
        call(path.clone(), Some(&key.key), same_baseline).0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut hidden_document = body.clone();
    hidden_document["analysis"]["purpose"] = json!("requirement_connection");
    hidden_document["requirement"] =
        json!({"document_id":"not-readable","version_id":"not-readable"});
    assert_eq!(
        call(path.clone(), Some(&key.key), hidden_document).0,
        StatusCode::NOT_FOUND
    );
    let mut injection = body.clone();
    injection["endpoint"] = json!("http://attacker");
    assert_eq!(
        call(path, Some(&key.key), injection).0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mcp = call(
        "/mcp".into(),
        Some(&key.key),
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nexora_evaluate_repository","arguments":{"project_id":project,"repository_id":repository,"analysis":body["analysis"]}}}),
    );
    assert_eq!(mcp.0, StatusCode::OK);
    assert_eq!(mcp.1["result"]["isError"], true);
    f.work
        .store
        .revoke_key(&f.work.ctx, &key.metadata.id)
        .unwrap();
    let revoked_path = format!(
        "/api/v1/projects/{}/repositories/{}/analyses",
        f.work.project.id, published.repository_id
    );
    assert_eq!(
        call(revoked_path, Some(&key.key), body).0,
        StatusCode::UNAUTHORIZED
    );
    let current = f
        .runtime
        .query_header(
            &f.header.tenant_id,
            &f.header.repository_id,
            Some(&f.header.id),
        )
        .unwrap();
    assert_eq!(original.payload_hash, current.payload_hash);
    let receipts_after: i64 = f
        .sql
        .query_one("SELECT count(*) FROM request_receipts", &[])
        .unwrap()
        .get(0);
    assert_eq!(
        receipts_before, receipts_after,
        "inference failures must not create business mutation receipts"
    );
}
