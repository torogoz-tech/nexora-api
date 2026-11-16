use nexora_api::{ApiServer, ApiState};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires local PostgreSQL; actual loopback HTTP and isolated installation"]
async fn http_authentication_scopes_and_revocation_use_real_actor_credentials() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_http_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let password = SecretGenerator::token("test_password").unwrap();
    let setup_config = config.clone();
    let setup_namespace = namespace.clone();
    let setup_password = password.clone();
    tokio::task::spawn_blocking(move || {
        let mut store = PlatformStore::connect(&setup_config, &setup_namespace).unwrap();
        store.initialize().unwrap();
        store
            .bootstrap("HTTP test", "owner", &setup_password, "Test owner")
            .unwrap();
    })
    .await
    .unwrap();
    let state = ApiState::new(config, namespace, 4).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, ApiServer::router(state))
            .await
            .unwrap()
    });
    tokio::task::spawn_blocking(move||{
        let client:ureq::Agent=ureq::Agent::config_builder().proxy(None).http_status_as_error(false).build().into();
        let mut contract=client.get(format!("{base}/openapi.json")).call().unwrap();
        assert_eq!(contract.status().as_u16(),200);
        let contract=contract.body_mut().read_json::<Value>().unwrap();
        assert_eq!(contract["openapi"],"3.1.0");
        assert!(contract["paths"]["/api/v1/actors/{id}/api-keys"]["post"].is_object());
        assert_eq!(client.get(format!("{base}/ready")).call().unwrap().status().as_u16(),200);
        let mut response=client.get(format!("{base}/api/v1/projects")).header("x-user-id","owner").call().unwrap();assert_eq!(response.status().as_u16(),401);assert_eq!(response.body_mut().read_json::<Value>().unwrap()["code"],"UNAUTHORIZED");
        let mut response=client.post(format!("{base}/api/v1/auth/login")).send_json(json!({"username":"owner","password":password})).unwrap();assert_eq!(response.status().as_u16(),200);let session=response.body_mut().read_json::<Value>().unwrap()["access_token"].as_str().unwrap().to_owned();let bearer=format!("Bearer {session}");
        let mut response=client.post(format!("{base}/api/v1/projects")).header("Authorization",&bearer).header("Idempotency-Key","project-once").send_json(json!({"name":"Allowed project"})).unwrap();assert_eq!(response.status().as_u16(),201);let project=response.body_mut().read_json::<Value>().unwrap()["id"].as_str().unwrap().to_owned();
        let mut replay=client.post(format!("{base}/api/v1/projects")).header("Authorization",&bearer).header("Idempotency-Key","project-once").send_json(json!({"name":"Allowed project"})).unwrap();
        assert_eq!(replay.status().as_u16(),201);
        assert_eq!(replay.body_mut().read_json::<Value>().unwrap()["id"],project);
        assert_eq!(client.post(format!("{base}/api/v1/projects")).header("Authorization",&bearer).header("Idempotency-Key","project-once").send_json(json!({"name":"Different input"})).unwrap().status().as_u16(),409);
        let mut response=client.post(format!("{base}/api/v1/projects")).header("Authorization",&bearer).send_json(json!({"name":"Other project"})).unwrap();assert_eq!(response.status().as_u16(),201);let other=response.body_mut().read_json::<Value>().unwrap()["id"].as_str().unwrap().to_owned();
        let mut response=client.post(format!("{base}/api/v1/actors")).header("Authorization",&bearer).send_json(json!({"kind":"agent","name":"Developer agent","purpose":"HTTP scope test"})).unwrap();assert_eq!(response.status().as_u16(),201);let actor=response.body_mut().read_json::<Value>().unwrap()["id"].as_str().unwrap().to_owned();
        let response=client.post(format!("{base}/api/v1/actors/{actor}/grants")).header("Authorization",&bearer).send_json(json!({"capability":"projects:read","project_id":project})).unwrap();assert_eq!(response.status().as_u16(),204);
        let mut response=client.post(format!("{base}/api/v1/actors/{actor}/api-keys")).header("Authorization",&bearer).send_json(json!({"name":"external client","scope":{"capabilities":["projects:read"],"project_ids":[project],"all_projects":false,"installation":false},"expires_in_seconds":3600})).unwrap();assert_eq!(response.status().as_u16(),201);let issued=response.body_mut().read_json::<Value>().unwrap();let key=issued["key"].as_str().unwrap().to_owned();let key_id=issued["metadata"]["id"].as_str().unwrap();let limited=format!("Bearer {key}");
        let mut response=client.get(format!("{base}/api/v1/projects")).header("Authorization",&limited).call().unwrap();assert_eq!(response.status().as_u16(),200);let items=response.body_mut().read_json::<Value>().unwrap();assert_eq!(items["items"].as_array().unwrap().len(),1);assert_eq!(items["items"][0]["id"],project);
        assert_eq!(client.get(format!("{base}/api/v1/projects/{other}")).header("Authorization",&limited).header("x-user-id","owner").call().unwrap().status().as_u16(),404);
        assert_eq!(client.post(format!("{base}/api/v1/actors")).header("Authorization",&limited).send_json(json!({"kind":"agent","name":"Forbidden","purpose":"No privileges"})).unwrap().status().as_u16(),403);
        let mut response=client.get(format!("{base}/api/v1/actors/{actor}/api-keys")).header("Authorization",&bearer).call().unwrap();assert_eq!(response.status().as_u16(),200);let text=response.body_mut().read_to_string().unwrap();assert!(!text.contains(&key));assert!(!text.contains("digest"));
        assert_eq!(client.delete(format!("{base}/api/v1/api-keys/{key_id}")).header("Authorization",&bearer).call().unwrap().status().as_u16(),204);
        assert_eq!(client.get(format!("{base}/api/v1/projects")).header("Authorization",&limited).call().unwrap().status().as_u16(),401);
        assert_eq!(client.post(format!("{base}/api/v1/auth/logout")).header("Authorization",&bearer).send_empty().unwrap().status().as_u16(),204);
        assert_eq!(client.get(format!("{base}/api/v1/projects")).header("Authorization",&bearer).call().unwrap().status().as_u16(),401);
    }).await.unwrap();
    server.abort();
    assert!(server.await.is_err_and(|error| error.is_cancelled()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires local PostgreSQL; verifies safe pool shutdown inside HTTP runtime"]
async fn connection_pool_can_close_inside_async_server_context() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_close_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let state = ApiState::new(config, namespace, 1).await.unwrap();
    drop(state);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires local PostgreSQL; an uninitialized installation must not be reported ready"]
async fn readiness_requires_migrations_and_an_initialized_organization() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_unready_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let state = ApiState::new(config, namespace, 1).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/ready", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, ApiServer::router(state))
            .await
            .unwrap()
    });
    tokio::task::spawn_blocking(move || {
        let client: ureq::Agent = ureq::Agent::config_builder()
            .proxy(None)
            .http_status_as_error(false)
            .build()
            .into();
        let mut result = client.get(url).call().unwrap();
        assert_eq!(result.status().as_u16(), 503);
        assert_eq!(
            result.body_mut().read_json::<Value>().unwrap()["ready"],
            false
        );
    })
    .await
    .unwrap();
    server.abort();
    assert!(server.await.is_err_and(|error| error.is_cancelled()));
}
