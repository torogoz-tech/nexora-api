use nexora_api::{ApiServer, ApiState};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires PostgreSQL; actual HTTP registration and MCP repository catalog"]
async fn http_and_mcp_share_repository_inscriptions_without_granting_code_access() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_rephttp_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let c = config.clone();
    let n = namespace.clone();
    let (session, project, other, key) = tokio::task::spawn_blocking(move || {
        use nexora_platform::{ActorKind, Capability, KeyScope};
        let mut store = PlatformStore::connect(&c, &n).unwrap();
        store.initialize().unwrap();
        let password = SecretGenerator::token("test_password").unwrap();
        store
            .bootstrap("Repository HTTP", "owner", &password, "Owner")
            .unwrap();
        let session = store.login("owner", &password).unwrap();
        let owner = store.authenticate(&session.access_token).unwrap();
        let project = store.create_project(&owner, "Multi repo", None).unwrap();
        let other = store.create_project(&owner, "Other", None).unwrap();
        let actor = store
            .create_actor(
                &owner,
                ActorKind::Agent,
                "Repository client",
                "HTTP fixture",
            )
            .unwrap();
        for capability in [Capability::RepositoriesRead, Capability::RepositoriesWrite] {
            store
                .grant(&owner, &actor.id, capability, Some(&project.id))
                .unwrap();
        }
        let key = store
            .issue_key(
                &owner,
                &actor.id,
                "Repository MCP",
                KeyScope {
                    capabilities: vec![Capability::RepositoriesRead, Capability::RepositoriesWrite],
                    project_ids: vec![project.id.clone()],
                    all_projects: false,
                    installation: false,
                },
                3600,
            )
            .unwrap();
        (session.access_token, project.id, other.id, key.key)
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
        let owner=format!("Bearer {session}");let actor=format!("Bearer {key}");
        let registration=json!({"provider":"github","full_name":"example/backend"});
        let mut response=client.post(format!("{base}/api/v1/projects/{project}/repositories")).header("Authorization",&actor).header("Idempotency-Key","registration-once").send_json(&registration).unwrap();assert_eq!(response.status().as_u16(),201);
        let first=response.body_mut().read_json::<Value>().unwrap();let id=first["id"].as_str().unwrap();assert_eq!(first["authorization_status"],"awaiting_authorization");assert!(first["remote_id"].is_null());assert!(first["connection_id"].is_null());
        let mut replay=client.post(format!("{base}/api/v1/projects/{project}/repositories")).header("Authorization",&actor).header("Idempotency-Key","registration-once").send_json(&registration).unwrap();assert_eq!(replay.status().as_u16(),201);assert_eq!(replay.body_mut().read_json::<Value>().unwrap()["id"],id);
        assert_eq!(client.post(format!("{base}/api/v1/projects/{project}/repositories")).header("Authorization",&owner).send_json(json!({"provider":"github","full_name":"example/unsafe","authorization_status":"authorized","remote_id":"123"})).unwrap().status().as_u16(),422);
        assert_eq!(client.get(format!("{base}/api/v1/projects/{other}/repositories/{id}")).header("Authorization",&actor).call().unwrap().status().as_u16(),404);
        let mut response=client.post(format!("{base}/mcp")).header("Authorization",&actor).header("Accept","application/json, text/event-stream").header("MCP-Protocol-Version","2025-11-25").send_json(json!({"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"nexora_repository_register","arguments":{"project_id":project,"registration":registration,"request_id":"registration-once"}}})).unwrap();
        assert_eq!(response.body_mut().read_json::<Value>().unwrap()["result"]["structuredContent"]["id"],id);
        let args=json!({"project_id":project,"registration":{"provider":"gitlab","full_name":"example/subgroup/frontend"},"request_id":"mcp-register-once"});
        let mut response=client.post(format!("{base}/mcp")).header("Authorization",&actor).header("Accept","application/json, text/event-stream").header("MCP-Protocol-Version","2025-11-25").send_json(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nexora_repository_register","arguments":args}})).unwrap();
        let reply=response.body_mut().read_json::<Value>().unwrap();assert_eq!(reply["result"]["isError"],false);assert_eq!(reply["result"]["structuredContent"]["authorization_status"],"awaiting_authorization");
        let mut response=client.post(format!("{base}/mcp")).header("Authorization",&actor).header("Accept","application/json, text/event-stream").header("MCP-Protocol-Version","2025-11-25").send_json(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nexora_repositories_list","arguments":{"project_id":project,"limit":1}}})).unwrap();
        let page=response.body_mut().read_json::<Value>().unwrap();assert_eq!(page["result"]["structuredContent"]["items"].as_array().unwrap().len(),1);assert!(page["result"]["structuredContent"]["next_cursor"].is_string());
        let mut response=client.get(format!("{base}/api/v1/projects/{project}/repositories")).header("Authorization",&actor).call().unwrap();assert_eq!(response.body_mut().read_json::<Value>().unwrap()["items"].as_array().unwrap().len(),2);
        let mut response=client.put(format!("{base}/api/v1/projects/{project}/repositories/{id}")).header("Authorization",&owner).header("Idempotency-Key","archive-once").send_json(json!({"expected_revision":1,"archived":true})).unwrap();assert_eq!(response.status().as_u16(),200);assert_eq!(response.body_mut().read_json::<Value>().unwrap()["revision"],2);
        let mut response=client.get(format!("{base}/api/v1/projects/{project}/repositories")).header("Authorization",&actor).call().unwrap();assert_eq!(response.body_mut().read_json::<Value>().unwrap()["items"].as_array().unwrap().len(),1);
        let mut response=client.post(format!("{base}/mcp")).header("Authorization",&actor).header("Accept","application/json, text/event-stream").header("MCP-Protocol-Version","2025-11-25").send_json(json!({"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"nexora_repository_get","arguments":{"project_id":project,"repository_id":id}}})).unwrap();
        assert_eq!(response.body_mut().read_json::<Value>().unwrap()["result"]["structuredContent"]["archived"],true);
        let mut response=client.post(format!("{base}/mcp")).header("Authorization",&actor).header("Accept","application/json, text/event-stream").header("MCP-Protocol-Version","2025-11-25").send_json(json!({"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"nexora_repository_change","arguments":{"project_id":project,"repository_id":id,"request_id":"restore-once","change":{"expected_revision":2,"archived":false}}}})).unwrap();
        assert_eq!(response.body_mut().read_json::<Value>().unwrap()["result"]["structuredContent"]["revision"],3);
        assert_eq!(client.get(format!("{base}/api/v1/projects/{project}/repositories?limit=abc")).header("Authorization",&actor).call().unwrap().status().as_u16(),422);
    }).await.unwrap();
    server.abort();
    assert!(server.await.is_err_and(|e| e.is_cancelled()));
}
