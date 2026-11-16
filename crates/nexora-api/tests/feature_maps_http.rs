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
use serde_json::{Value, json};
use tower::ServiceExt;
#[test]
#[ignore = "requires PostgreSQL; feature REST/MCP schemas, scoped reads, proposal retry and human-only decisions"]
fn feature_http_and_mcp_preserve_role_scope_and_human_decisions() {
    let mut f = FeatureMapFixture::new();
    let developer_key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.developer.actor_id(),
            "Feature contributor",
            KeyScope {
                capabilities: MemberRole::Developer.capabilities(),
                project_ids: vec![f.work.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let admin_key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Installation API",
            KeyScope {
                capabilities: Capability::all(),
                project_ids: vec![],
                all_projects: true,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let pmo_key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.pmo.actor_id(),
            "Scoped PMO API",
            KeyScope {
                capabilities: MemberRole::Pmo.capabilities(),
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
        .create_project(&f.work.ctx, "Other project scope", None)
        .unwrap();
    let executor = tokio::runtime::Runtime::new().unwrap();
    let app = executor.block_on(async {
        ApiServer::router(
            ApiState::new(f.work.config.clone(), f.work.namespace.clone(), 4)
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
    let rpc = |token: &str, name: &str, args: Value| {
        request(
            "POST",
            "/mcp",
            token,
            Some(
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}),
            ),
            None,
        )
    };
    let (status, catalog) = request(
        "GET",
        &format!("/api/v1/projects/{}/features", f.work.project.id),
        &developer_key.key,
        None,
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(catalog["catalog_revision"], 0);
    assert_eq!(catalog["items"], json!([]));
    assert_eq!(catalog["can_decide"], false);
    assert_eq!(
        request(
            "GET",
            &format!("/api/v1/projects/{}/features", other.id),
            &developer_key.key,
            None,
            None
        )
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, tools) = request(
        "POST",
        "/mcp",
        &developer_key.key,
        Some(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}})),
        None,
    );
    assert_eq!(status, StatusCode::OK);
    let tools = tools["result"]["tools"].as_array().unwrap();
    let read = tools
        .iter()
        .find(|t| t["name"] == "nexora_features_list")
        .unwrap();
    assert_eq!(read["inputSchema"]["required"], json!(["project_id"]));
    assert_eq!(read["inputSchema"]["additionalProperties"], false);
    let propose = tools
        .iter()
        .find(|t| t["name"] == "nexora_feature_map_propose")
        .unwrap();
    assert_eq!(
        propose["inputSchema"]["required"],
        json!(["project_id", "request_id", "proposal"])
    );
    assert_eq!(
        propose["inputSchema"]["properties"]["proposal"]["properties"]["tasks"]["items"]["properties"]
            ["feature_index"]["minimum"],
        0
    );
    assert!(
        tools
            .iter()
            .all(|t| t["name"] != "nexora_feature_map_decide")
    );
    let (_, catalog) = rpc(
        &developer_key.key,
        "nexora_features_list",
        json!({"project_id":f.work.project.id}),
    );
    assert_eq!(catalog["result"]["isError"], false);
    assert_eq!(
        catalog["result"]["structuredContent"]["catalog_revision"],
        0
    );
    let (_, denied) = rpc(
        &developer_key.key,
        "nexora_features_list",
        json!({"project_id":other.id}),
    );
    assert_eq!(denied["result"]["isError"], true);
    let role_path = format!("/api/v1/settings/members/{}/role", f.developer.actor_id());
    let (status, role) = request("GET", &role_path, &admin_key.key, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(role["role"], "developer");
    assert_eq!(role["revision"], 1);
    assert_eq!(role["project_ids"], json!([f.work.project.id]));
    assert_eq!(
        request("GET", &role_path, &developer_key.key, None, None).0,
        StatusCode::FORBIDDEN
    );
    let change =
        json!({"role":"manager","all_projects":true,"project_ids":[],"expected_revision":1});
    assert_eq!(
        request(
            "PUT",
            &role_path,
            &admin_key.key,
            Some(change.clone()),
            None
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        request(
            "PUT",
            &role_path,
            &admin_key.key,
            Some(change),
            Some("api-cannot-change-human-role")
        )
        .0,
        StatusCode::FORBIDDEN
    );
    let input = serde_json::to_value(f.proposal("Governed HTTP capability", false)).unwrap();
    let proposal_path = format!("/api/v1/projects/{}/feature-proposals", f.work.project.id);
    let (status, proposed) = request(
        "POST",
        &proposal_path,
        &developer_key.key,
        Some(input.clone()),
        Some("same-proposal-rest-mcp"),
    );
    assert_eq!(status, StatusCode::CREATED, "{proposed}");
    let (_, retry) = rpc(
        &developer_key.key,
        "nexora_feature_map_propose",
        json!({"project_id":f.work.project.id,"request_id":"same-proposal-rest-mcp","proposal":input}),
    );
    assert_eq!(retry["result"]["isError"], false, "{retry}");
    assert_eq!(retry["result"]["structuredContent"]["id"], proposed["id"]);
    let decide_path = format!(
        "{}/{}/decision",
        proposal_path,
        proposed["id"].as_str().unwrap()
    );
    let decision = serde_json::to_value(FeatureMapFixture::decision(true)).unwrap();
    assert_eq!(
        request(
            "POST",
            &decide_path,
            &pmo_key.key,
            Some(decision.clone()),
            Some("api-cannot-decide")
        )
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            "POST",
            &decide_path,
            "missing-token",
            Some(decision),
            Some("invalid-session")
        )
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(f.counts(), (0, 0, 0, 0, 0));
}
