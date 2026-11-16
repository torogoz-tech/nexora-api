use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::{Value, json};
use std::path::Path;
use tower::ServiceExt;

async fn request(
    app: &axum::Router,
    method: &str,
    path: &str,
    token: &str,
    body: Option<Value>,
    id: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {token}"));
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    if let Some(id) = id {
        builder = builder.header("idempotency-key", id);
    }
    if path == "/mcp" {
        builder = builder
            .header("accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2025-11-25");
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
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
#[test]
#[ignore = "requires PostgreSQL; personal profile keys and successful MCP activity"]
fn profile_key_lifecycle_and_transport_activity_are_distinct_from_rest_use() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = SecretGenerator::id("nexora_platform_mcp_profile").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = zeroize::Zeroizing::new(SecretGenerator::token("fixture").unwrap());
    store.bootstrap("MCP", "owner", &password, "Owner").unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let project = store.create_project(&ctx, "Profile", None).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async{
        let app=ApiServer::router(ApiState::new(config,namespace,4).await.unwrap());
        let input=json!({"name":"Editor","project_ids":[project.id],"read_only":true,"include_global":false,"expires_in_seconds":86400});
        let (status,issued)=request(&app,"POST","/api/v1/me/mcp-keys",&session.access_token,Some(input.clone()),Some("key-create")).await;assert_eq!(status,StatusCode::CREATED);
        let key=issued["key"].as_str().unwrap();let id=issued["metadata"]["id"].as_str().unwrap();
        assert_eq!(request(&app,"POST","/api/v1/me/mcp-keys",&session.access_token,Some(input),Some("key-create")).await.0,StatusCode::CONFLICT);
        assert_eq!(request(&app,"GET","/api/v1/me/mcp-keys",key,None,None).await.0,StatusCode::FORBIDDEN);
        let (_,listed)=request(&app,"GET","/api/v1/me/mcp-keys",&session.access_token,None,None).await;assert!(listed["items"][0]["last_mcp_at"].is_null());assert!(!listed.to_string().contains(key));
        let invalid=json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}});
        request(&app,"POST","/mcp",key,Some(invalid),None).await;
        let (_,listed)=request(&app,"GET","/api/v1/me/mcp-keys",&session.access_token,None,None).await;assert!(listed["items"][0]["last_mcp_at"].is_null());
        let initialize=json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"Editor fixture","version":"1"}}});
        let (status,result)=request(&app,"POST","/mcp",key,Some(initialize),None).await;assert_eq!(status,StatusCode::OK);assert!(result.get("result").is_some());
        let (_,listed)=request(&app,"GET","/api/v1/me/mcp-keys",&session.access_token,None,None).await;assert!(listed["items"][0]["last_mcp_at"].is_string());assert_eq!(listed["items"][0]["mcp_client_name"],"Editor fixture");
        assert_eq!(request(&app,"DELETE",&format!("/api/v1/me/mcp-keys/{id}"),&session.access_token,None,Some("key-revoke")).await.0,StatusCode::NO_CONTENT);
        assert_eq!(request(&app,"POST","/mcp",key,Some(json!({"jsonrpc":"2.0","id":3,"method":"ping"})),None).await.0,StatusCode::UNAUTHORIZED);
        let (status,history)=request(&app,"GET","/api/v1/me/mcp-keys?status=inactive&limit=20",&session.access_token,None,None).await;
        assert_eq!(status,StatusCode::OK);assert_eq!(history["items"].as_array().unwrap().len(),1);assert_eq!(history["has_more"],false);assert!(history["next_after"].is_null());
        let (_,active)=request(&app,"GET","/api/v1/me/mcp-keys?status=active&limit=20",&session.access_token,None,None).await;assert!(active["items"].as_array().unwrap().is_empty());
        assert_eq!(request(&app,"GET","/api/v1/me/mcp-keys?status=inactive&limit=21",&session.access_token,None,None).await.0,StatusCode::UNPROCESSABLE_ENTITY);

    });
}
