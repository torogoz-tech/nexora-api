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
#[ignore = "requires PostgreSQL"]
fn team_calendar_http_lifecycle() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = SecretGenerator::id("nexora_platform_calendar_http").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = zeroize::Zeroizing::new(SecretGenerator::token("fixture").unwrap());
    store
        .bootstrap("Team calendar", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
  let app=ApiServer::router(ApiState::new(config,namespace,4).await.unwrap());
  let token=&session.access_token;
  let input=json!({"title":"Planning","start_date":"2026-11-10","end_date":"2026-11-10","all_day":true});
  let path="/api/v1/calendar/activities";
  assert_eq!(request(&app,"POST",path,token,Some(input.clone()),None).await.0,StatusCode::UNPROCESSABLE_ENTITY);
  let(status,created)=request(&app,"POST",path,token,Some(input.clone()),Some("create-event")).await;assert_eq!(status,StatusCode::OK);
  let (_,replayed)=request(&app,"POST",path,token,Some(input.clone()),Some("create-event")).await;assert_eq!(replayed["id"],created["id"]);
  let list="/api/v1/calendar/activities?from=2026-11-01&to=2026-11-30&limit=20";
  let(status,page)=request(&app,"GET",list,token,None,None).await;assert_eq!(status,StatusCode::OK);assert_eq!(page["items"].as_array().unwrap().len(),1);
  let id=created["id"].as_str().unwrap();let mut update=input.clone();update["expected_revision"]=json!(0);update["title"]=json!("New title");
  assert_eq!(request(&app,"PUT",&format!("{path}/{id}"),token,Some(update.clone()),Some("change-event")).await.0,StatusCode::OK);
  assert_eq!(request(&app,"PUT",&format!("{path}/{id}"),token,Some(update),Some("stale-event")).await.0,StatusCode::CONFLICT);
  assert_eq!(request(&app,"POST",&format!("{path}/{id}/cancel"),token,Some(json!({"expected_revision":1})),Some("cancel-event")).await.0,StatusCode::OK);
  assert!(request(&app,"GET",list,token,None,None).await.1["items"].as_array().unwrap().is_empty());
 });
}
