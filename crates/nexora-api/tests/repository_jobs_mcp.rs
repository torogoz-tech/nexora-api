#[path = "../../nexora-platform-store/tests/support/published_repository_fixture.rs"]
mod published_repository_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::{Capability, GitProvider, KeyScope, RepositoryRegistration};
use published_repository_fixture::PublishedRepositoryFixture;
use serde_json::{Value, json};
use tower::ServiceExt;

#[test]
#[ignore = "requires PostgreSQL/Dragonfly/Neo4j; registered MCP job controls reuse durable REST services and receipts without arbitrary roots"]
fn mcp_repository_jobs_share_registered_source_authority_and_rest_receipts() {
    let mut f = PublishedRepositoryFixture::new();
    let project = f.work.project.id.clone();
    let repository = f.binding.repository_id.clone();
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Jobs MCP",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead, Capability::RepositoriesSync],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let readonly = f.contributor("Jobs readonly", false);
    let reader = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            readonly.actor_id(),
            "Jobs MCP readonly",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let executor = tokio::runtime::Runtime::new().unwrap();
    let app = executor.block_on(async {
        ApiServer::router(
            ApiState::new(f.work.config.clone(), f.work.namespace.clone(), 2)
                .await
                .unwrap(),
        )
    });
    let request = |method: &str, path: &str, token: &str, body: Option<Value>, id: Option<&str>| {
        executor.block_on(async {
            let mut builder = Request::builder()
                .method(method)
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .header("mcp-protocol-version", "2025-11-25");
            if let Some(id) = id {
                builder = builder.header("idempotency-key", id);
            }
            let response = app
                .clone()
                .oneshot(
                    builder
                        .body(Body::from(
                            body.map(|v| serde_json::to_vec(&v).unwrap())
                                .unwrap_or_default(),
                        ))
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
    let rpc = |token: &str, name: &str, arguments: Value| {
        request(
            "POST",
            "/mcp",
            token,
            Some(
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":arguments}}),
            ),
            None,
        )
    };
    let (_, catalog) = request(
        "POST",
        "/mcp",
        &key.key,
        Some(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})),
        None,
    );
    let tools = catalog["result"]["tools"].as_array().unwrap();
    for name in [
        "nexora_sync_repository",
        "nexora_index_repository",
        "nexora_repository_job_get",
        "nexora_repository_job_cancel",
    ] {
        assert!(
            tools.iter().any(|tool| tool["name"] == name),
            "missing registered job tool: {name}"
        );
    }
    let input = json!({"project_id":project,"repository_id":repository,"request_id":"index-mcp","request":{"expected_revision":2}});
    let (_, queued) = rpc(&key.key, "nexora_index_repository", input.clone());
    assert_eq!(queued["result"]["isError"], false);
    let indexed = queued["result"]["structuredContent"].clone();
    assert_eq!(indexed["state"], "queued");
    assert_eq!(indexed["kind"], "index");
    assert_eq!(indexed["requested_by"], f.work.ctx.actor_id());
    assert!(indexed.get("lease_token_digest").is_none());
    assert!(indexed.get("tokens").is_none());
    let job = indexed["id"].as_str().unwrap();
    let repository_path = format!("/api/v1/projects/{project}/repositories/{repository}");
    let (status, replay) = request(
        "POST",
        &format!("{repository_path}/index"),
        &key.key,
        Some(json!({"expected_revision":2})),
        Some("index-mcp"),
    );
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(replay, indexed);
    assert_eq!(
        rpc(&reader.key, "nexora_index_repository", input.clone()).1["result"]["isError"],
        true
    );
    let lookup = json!({"project_id":project,"job_id":job});
    assert_eq!(
        rpc(&reader.key, "nexora_repository_job_get", lookup.clone()).1["result"]["structuredContent"],
        indexed
    );
    let cancel = json!({"project_id":project,"job_id":job,"request_id":"cancel-mcp","change":{"expected_revision":1}});
    let cancelled = rpc(&key.key, "nexora_repository_job_cancel", cancel.clone()).1["result"]["structuredContent"].clone();
    assert_eq!(cancelled["state"], "cancelled");
    let job_path = format!("/api/v1/projects/{project}/jobs/{job}");
    let (status, replay) = request(
        "POST",
        &format!("{job_path}/cancel"),
        &key.key,
        Some(json!({"expected_revision":1})),
        Some("cancel-mcp"),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay, cancelled);
    let (status, sync) = request(
        "POST",
        &format!("{repository_path}/sync"),
        &key.key,
        Some(json!({"expected_revision":2})),
        Some("sync-http"),
    );
    assert_eq!(status, StatusCode::ACCEPTED);
    let mut sync_input = input.clone();
    sync_input["request_id"] = json!("sync-http");
    assert_eq!(
        rpc(&key.key, "nexora_sync_repository", sync_input).1["result"]["structuredContent"],
        sync
    );
    let sync_job = sync["id"].as_str().unwrap();
    let (status, sync_cancelled) = request(
        "POST",
        &format!("/api/v1/projects/{project}/jobs/{sync_job}/cancel"),
        &key.key,
        Some(json!({"expected_revision":1})),
        Some("cancel-http"),
    );
    assert_eq!(status, StatusCode::OK);
    let sync_cancel = json!({"project_id":project,"job_id":sync_job,"request_id":"cancel-http","change":{"expected_revision":1}});
    assert_eq!(
        rpc(&key.key, "nexora_repository_job_cancel", sync_cancel).1["result"]["structuredContent"],
        sync_cancelled
    );
    let mut invalid = input.clone();
    invalid["root"] = json!("/client-selected-root");
    assert_eq!(
        rpc(&key.key, "nexora_index_repository", invalid).1["error"]["code"],
        -32602
    );
    let mut foreign = lookup.clone();
    foreign["project_id"] = json!("another-project");
    assert_eq!(
        rpc(&key.key, "nexora_repository_job_get", foreign).1["result"]["isError"],
        true
    );
    let awaiting = f
        .work
        .store
        .register_repository(
            &f.work.ctx,
            &project,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "fixture/not-authorized".into(),
            },
        )
        .unwrap();
    let awaiting_input = json!({"project_id":project,"repository_id":awaiting.id,"request_id":"awaiting-source","request":{"expected_revision":1}});
    assert_eq!(
        rpc(&key.key, "nexora_index_repository", awaiting_input).1["result"]["isError"],
        true
    );
    let previous_snapshot = f.binding.snapshot_id.clone();
    let next = f.publish_revision(2);
    assert_ne!(next.snapshot_id, previous_snapshot);
    assert_eq!(
        rpc(&reader.key, "nexora_repository_job_get", lookup.clone()).1["result"]["structuredContent"],
        cancelled
    );
    f.sql
        .execute(
            "UPDATE git_connections SET status='revoked',revision=revision+1 WHERE id=$1",
            &[&f.connection],
        )
        .unwrap();
    let mut revoked = input;
    revoked["request_id"] = json!("revoked-source");
    assert_eq!(
        rpc(&key.key, "nexora_index_repository", revoked).1["result"]["isError"],
        true
    );
    f.work
        .store
        .revoke_key(&f.work.ctx, &key.metadata.id)
        .unwrap();
    assert_eq!(
        rpc(&key.key, "nexora_repository_job_get", lookup).0,
        StatusCode::UNAUTHORIZED
    );
    let mut restarted =
        nexora_platform_store::PlatformStore::connect(&f.work.config, &f.work.namespace).unwrap();
    assert_eq!(
        restarted
            .repository_job(&f.work.ctx, &project, job)
            .unwrap()
            .state,
        nexora_platform::RepositoryJobState::Cancelled
    );
    std::fs::remove_dir_all(&f.root).unwrap();
}
