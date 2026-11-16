use nexora_api::{ApiServer, ApiState};
use nexora_platform::{ActorKind, Capability, KeyScope};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires local PostgreSQL; installation MCP authenticates every request and isolates actors"]
async fn installation_mcp_uses_individual_keys_and_current_project_permissions() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_mcp_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let c = config.clone();
    let n = namespace.clone();
    let (owner_token, keys, projects) = tokio::task::spawn_blocking(move || {
        let mut store = PlatformStore::connect(&c, &n).unwrap();
        store.initialize().unwrap();
        let password = SecretGenerator::token("test_password").unwrap();
        store
            .bootstrap("MCP fixture", "owner", &password, "Owner")
            .unwrap();
        let session = store.login("owner", &password).unwrap();
        let ctx = store.authenticate(&session.access_token).unwrap();
        let mut projects = vec![];
        let mut keys = vec![];
        for name in ["Project A", "Project B"] {
            let project = store.create_project(&ctx, name, None).unwrap();
            let actor = store
                .create_actor(&ctx, ActorKind::Agent, name, "External developer fixture")
                .unwrap();
            store
                .grant(&ctx, &actor.id, Capability::ProjectsRead, Some(&project.id))
                .unwrap();
            keys.push(
                store
                    .issue_key(
                        &ctx,
                        &actor.id,
                        "MCP fixture",
                        KeyScope {
                            capabilities: vec![Capability::ProjectsRead],
                            project_ids: vec![project.id.clone()],
                            all_projects: false,
                            installation: false,
                        },
                        3600,
                    )
                    .unwrap(),
            );
            projects.push(project);
        }
        (session.access_token, keys, projects)
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
        let initialize=json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"acceptance-test","version":"1"}}});
        assert_eq!(client.post(format!("{base}/mcp")).header("Accept","application/json, text/event-stream").send_json(&initialize).unwrap().status().as_u16(),401);
        assert_eq!(client.post(format!("{base}/mcp")).header("Authorization",format!("Bearer {owner_token}")).header("Accept","application/json, text/event-stream").send_json(&initialize).unwrap().status().as_u16(),403);
        let mut permissions = client.get(format!("{base}/api/v1/me/permissions")).header("Authorization",format!("Bearer {owner_token}")).call().unwrap();
        assert_eq!(permissions.status().as_u16(),200);
        assert_eq!(permissions.body_mut().read_json::<Value>().unwrap()["role"],"super_admin");
        for (i,key) in keys.iter().enumerate() {
            let bearer=format!("Bearer {}",key.key);
            let mut response=client.post(format!("{base}/mcp")).header("Authorization",&bearer).header("Accept","application/json, text/event-stream").send_json(&initialize).unwrap();
            assert_eq!(response.status().as_u16(),200);
            assert!(!response.headers().contains_key("Mcp-Session-Id"));
            assert_eq!(response.body_mut().read_json::<Value>().unwrap()["result"]["serverInfo"]["name"],"nexora");
            assert_eq!(client.post(format!("{base}/mcp")).header("Authorization",&bearer).header("Accept","application/json, text/event-stream").header("MCP-Protocol-Version","2025-11-25").send_json(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).unwrap().status().as_u16(),202);
            let mut response=client.post(format!("{base}/mcp")).header("Authorization",&bearer).header("Accept","application/json, text/event-stream").header("MCP-Protocol-Version","2025-11-25").send_json(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
            let catalog=response.body_mut().read_json::<Value>().unwrap();
            assert!(catalog["result"]["tools"].as_array().unwrap().iter().any(|t|t["name"]=="nexora_projects_list"));
            let mut permissions=client.post(format!("{base}/mcp")).header("Authorization",&bearer).header("Accept","application/json, text/event-stream").header("MCP-Protocol-Version","2025-11-25").send_json(json!({"jsonrpc":"2.0","id":22,"method":"tools/call","params":{"name":"nexora_permissions_get","arguments":{}}})).unwrap();
            let permissions=permissions.body_mut().read_json::<Value>().unwrap();
            assert_eq!(permissions["result"]["isError"],false);
            assert_eq!(permissions["result"]["structuredContent"]["grants"].as_array().unwrap().len(),1);
            assert_eq!(permissions["result"]["structuredContent"]["grants"][0]["capability"],"projects:read");
            assert_eq!(permissions["result"]["structuredContent"]["grants"][0]["project_id"],projects[i].id);
            let mut response=client.post(format!("{base}/mcp")).header("Authorization",&bearer).header("Accept","application/json, text/event-stream").header("MCP-Protocol-Version","2025-11-25").send_json(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nexora_projects_list","arguments":{"limit":1}}})).unwrap();
            assert_eq!(response.status().as_u16(),200);
            let result=response.body_mut().read_json::<Value>().unwrap();
            assert_eq!(result["result"]["isError"],false);
            assert_eq!(result["result"]["structuredContent"]["items"][0]["id"],projects[i].id);
            assert_eq!(result["result"]["structuredContent"]["items"].as_array().unwrap().len(),1);
            let mut response=client.post(format!("{base}/mcp")).header("Authorization",&bearer).header("Accept","application/json, text/event-stream").header("MCP-Protocol-Version","2025-11-25").send_json(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nexora_project_get","arguments":{"project_id":projects[1-i].id}}})).unwrap();
            assert_eq!(response.body_mut().read_json::<Value>().unwrap()["result"]["isError"],true);
            let mut response=client.post(format!("{base}/mcp")).header("Authorization",&bearer).header("Accept","application/json, text/event-stream").send_json(json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nexora_projects_list","arguments":{"actor_id":"owner"}}})).unwrap();
            assert_eq!(response.body_mut().read_json::<Value>().unwrap()["error"]["code"],-32602);
            assert_eq!(client.post(format!("{base}/mcp")).header("Authorization",&bearer).header("Accept","application/json, text/event-stream").header("Origin","https://untrusted.example").send_json(&initialize).unwrap().status().as_u16(),403);
            assert_eq!(client.post(format!("{base}/mcp")).header("Authorization",&bearer).header("Accept","application/json, text/event-stream").header("MCP-Protocol-Version","unknown").send_json(&initialize).unwrap().status().as_u16(),400);
            assert_eq!(client.get(format!("{base}/mcp")).header("Authorization",&bearer).call().unwrap().status().as_u16(),405);
        }
        assert_eq!(client.delete(format!("{base}/api/v1/api-keys/{}",keys[0].metadata.id)).header("Authorization",format!("Bearer {owner_token}")).call().unwrap().status().as_u16(),204);
        assert_eq!(client.post(format!("{base}/mcp")).header("Authorization",format!("Bearer {}",keys[0].key)).header("Accept","application/json, text/event-stream").send_json(json!({"jsonrpc":"2.0","id":6,"method":"tools/list"})).unwrap().status().as_u16(),401);
    }).await.unwrap();
    server.abort();
    assert!(server.await.is_err_and(|e| e.is_cancelled()));
}
