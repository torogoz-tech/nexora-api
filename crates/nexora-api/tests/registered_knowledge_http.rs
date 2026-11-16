#[path = "../../nexora-platform-store/tests/support/published_repository_fixture.rs"]
mod published_repository_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::{Capability, KeyScope};
use published_repository_fixture::PublishedRepositoryFixture;
use serde_json::{Value, json};
use tower::ServiceExt;

#[test]
#[ignore = "requires PostgreSQL/Dragonfly/Neo4j; registered knowledge REST/MCP share actor authority, immutable history and receipts"]
fn registered_knowledge_rest_and_mcp_share_authenticated_lifecycle_and_receipts() {
    let mut f = PublishedRepositoryFixture::new();
    let project = f.work.project.id.clone();
    let repository = f.binding.repository_id.clone();
    let writer = f.contributor("HTTP knowledge contributor", true);
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            writer.actor_id(),
            "Knowledge HTTP",
            KeyScope {
                capabilities: vec![
                    Capability::RepositoriesRead,
                    Capability::KnowledgeRead,
                    Capability::KnowledgeWrite,
                ],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let reader = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Knowledge readonly HTTP",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead, Capability::KnowledgeRead],
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
        "nexora_list_pending",
        "nexora_list_knowledge",
        "nexora_get_knowledge",
        "nexora_contribute_knowledge",
        "nexora_revise_knowledge",
        "nexora_review_knowledge",
    ] {
        assert!(
            tools.iter().any(|tool| tool["name"] == name),
            "missing registered knowledge tool: {name}"
        );
    }
    let base = format!("/api/v1/projects/{project}/repositories/{repository}");
    let path = format!("{base}/knowledge");
    let draft = json!({"kind":"decision","statement":"A developer agent supplied a decision with published source evidence.","evidence":[{"path":"source.js","start_line":1,"line_count":1}]});
    let body = json!({"draft":draft,"snapshot":Value::Null});
    assert_eq!(
        request(
            "POST",
            &path,
            &reader.key,
            Some(body.clone()),
            Some("read-denied")
        )
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request("POST", &path, &key.key, Some(body.clone()), None).0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, first) = request(
        "POST",
        &path,
        &key.key,
        Some(body.clone()),
        Some("cross-transport"),
    );
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(first["project_id"], project);
    assert_eq!(first["repository_id"], repository);
    assert_eq!(first["contribution"]["actor"], writer.actor_id());
    let scope = json!({"project_id":project,"repository_id":repository});
    let mut contribute = scope.clone();
    contribute["request_id"] = json!("cross-transport");
    contribute["draft"] = draft.clone();
    let (_, replay) = rpc(&key.key, "nexora_contribute_knowledge", contribute.clone());
    assert_eq!(replay["result"]["isError"], false);
    assert_eq!(replay["result"]["structuredContent"], first);
    let mut altered = contribute.clone();
    altered["draft"]["statement"] = json!("A different request cannot reuse the receipt.");
    let (_, conflict) = rpc(&key.key, "nexora_contribute_knowledge", altered);
    assert_eq!(conflict["result"]["isError"], true);
    let id = first["contribution"]["id"].as_str().unwrap();
    let item_path = format!("{path}/{id}");
    let revision_body = json!({"snapshot":Value::Null,"expected_revision":1,"draft":draft});
    let (status, revised) = request(
        "PATCH",
        &item_path,
        &key.key,
        Some(revision_body),
        Some("revise-http"),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(revised["contribution"]["revision"], 2);
    let mut revision_replay = scope.clone();
    revision_replay["id"] = json!(id);
    revision_replay["request_id"] = json!("revise-http");
    revision_replay["expected_revision"] = json!(1);
    revision_replay["draft"] = draft.clone();
    assert_eq!(
        rpc(&key.key, "nexora_revise_knowledge", revision_replay).1["result"]["structuredContent"],
        revised
    );
    for (revision, action) in [(2, "accept"), (3, "reject"), (4, "withdraw")] {
        let mut arguments = scope.clone();
        arguments["id"] = json!(id);
        arguments["request_id"] = json!(format!("review-{action}"));
        arguments["expected_revision"] = json!(revision);
        arguments["action"] = json!(action);
        arguments["reason"] = json!("The external client reviewed this contribution.");
        let http_review = if action == "accept" {
            let body = json!({"snapshot":Value::Null,"expected_revision":revision,"action":action,"reason":"The external client reviewed this contribution."});
            let (status, value) = request(
                "POST",
                &format!("{item_path}/review"),
                &key.key,
                Some(body),
                Some("review-accept"),
            );
            assert_eq!(status, StatusCode::OK);
            Some(value)
        } else {
            None
        };
        let (_, value) = rpc(&key.key, "nexora_review_knowledge", arguments);
        if let Some(http_review) = http_review {
            assert_eq!(value["result"]["structuredContent"], http_review);
        }
        assert_eq!(value["result"]["isError"], false);
        assert_eq!(
            value["result"]["structuredContent"]["contribution"]["revision"],
            revision + 1
        );
    }
    let (status, record) = request("GET", &item_path, &reader.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(record["history_total"], 5);
    assert_eq!(record["contribution"]["state"], "withdrawn");
    for name in ["nexora_list_knowledge", "nexora_list_pending"] {
        let (_, value) = rpc(&reader.key, name, scope.clone());
        assert_eq!(value["result"]["isError"], false);
        assert_eq!(
            value["result"]["structuredContent"]["repository_id"],
            repository
        );
    }
    let (status, listed) = request("GET", &format!("{path}?limit=1"), &reader.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        request(
            "GET",
            &format!("{base}/pending?limit=1"),
            &reader.key,
            None,
            None
        )
        .0,
        StatusCode::OK
    );
    let mut get = scope.clone();
    get["id"] = json!(id);
    let (_, value) = rpc(&reader.key, "nexora_get_knowledge", get.clone());
    assert_eq!(value["result"]["structuredContent"]["history_total"], 5);
    get["actor"] = json!("client-cannot-select-an-actor");
    let (_, invalid) = rpc(&key.key, "nexora_get_knowledge", get);
    assert_eq!(invalid["error"]["code"], -32602);
    assert_eq!(
        request(
            "GET",
            &format!("{base}/pending?limit=0"),
            &reader.key,
            None,
            None
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let next = f.publish_revision(2);
    assert_ne!(next.snapshot_id, first["snapshot"].as_str().unwrap());
    assert_eq!(
        rpc(&key.key, "nexora_contribute_knowledge", contribute.clone()).1["result"]["structuredContent"],
        first
    );
    f.sql
        .execute(
            "UPDATE git_connections SET status='revoked',revision=revision+1 WHERE id=$1",
            &[&f.connection],
        )
        .unwrap();
    assert_eq!(
        request("GET", &item_path, &reader.key, None, None).0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        rpc(&key.key, "nexora_contribute_knowledge", contribute).1["result"]["isError"],
        true
    );
    // Reconnect the platform store to retain the fixture's durable configuration contract.
    let mut restarted =
        nexora_platform_store::PlatformStore::connect(&f.work.config, &f.work.namespace).unwrap();
    assert!(
        restarted
            .query_repository_knowledge(
                &f.work.ctx,
                &project,
                &repository,
                None,
                &nexora_runtime::KnowledgeCommand::Get {
                    id: id.into(),
                    offset: 0,
                    limit: 1
                }
            )
            .is_err()
    );
    std::fs::remove_dir_all(&f.root).unwrap();
}
