use nexora_api::{ApiServer, ApiState};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires local PostgreSQL; actual HTTP project edits, archives and pagination"]
async fn project_http_contract_preserves_revisions_and_bounds_every_page() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_projhttp_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let c = config.clone();
    let n = namespace.clone();
    let token = tokio::task::spawn_blocking(move || {
        let mut store = PlatformStore::connect(&c, &n).unwrap();
        store.initialize().unwrap();
        let password = SecretGenerator::token("test_password").unwrap();
        store
            .bootstrap("Project HTTP", "owner", &password, "Owner")
            .unwrap();
        store.login("owner", &password).unwrap().access_token
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
        let bearer=format!("Bearer {token}");
        let mut ids=vec![];
        for name in ["One","Two","Three"] {
            let mut response=client.post(format!("{base}/api/v1/projects")).header("Authorization",&bearer).send_json(json!({"name":name})).unwrap();
            assert_eq!(response.status().as_u16(),201);
            ids.push(response.body_mut().read_json::<Value>().unwrap()["id"].as_str().unwrap().to_owned());
        }
        let id=ids[0].clone();
        let change=json!({"expected_revision":1,"name":"Renamed","description":"Architecture","archived":false});
        let mut response=client.put(format!("{base}/api/v1/projects/{id}")).header("Authorization",&bearer).header("Idempotency-Key","edit-once").send_json(&change).unwrap();
        assert_eq!(response.status().as_u16(),200);
        assert_eq!(response.body_mut().read_json::<Value>().unwrap()["revision"],2);
        let mut response=client.put(format!("{base}/api/v1/projects/{id}")).header("Authorization",&bearer).header("Idempotency-Key","edit-once").send_json(&change).unwrap();
        assert_eq!(response.status().as_u16(),200);
        assert_eq!(response.body_mut().read_json::<Value>().unwrap()["revision"],2);
        assert_eq!(client.put(format!("{base}/api/v1/projects/{id}")).header("Authorization",&bearer).send_json(&change).unwrap().status().as_u16(),409);
        let mut cursor=None;
        let mut seen=vec![];
        loop {
            let url=match cursor {Some(ref after)=>format!("{base}/api/v1/projects?limit=1&after={after}"),None=>format!("{base}/api/v1/projects?limit=1")};
            let mut response=client.get(url).header("Authorization",&bearer).call().unwrap();
            assert_eq!(response.status().as_u16(),200);
            let page=response.body_mut().read_json::<Value>().unwrap();
            assert_eq!(page["items"].as_array().unwrap().len(),1);
            seen.push(page["items"][0]["id"].as_str().unwrap().to_owned());
            cursor=page["next_cursor"].as_str().map(str::to_owned);
            if cursor.is_none(){break;}
        }
        ids.sort();seen.sort();assert_eq!(seen,ids);
        for query in ["limit=0","limit=101","limit=abc","after=invalid","include_archived=abc","unknown=true"] {
            assert_eq!(client.get(format!("{base}/api/v1/projects?{query}")).header("Authorization",&bearer).call().unwrap().status().as_u16(),422);
        }
        let archive=json!({"expected_revision":2,"name":"Renamed","description":null,"archived":true});
        assert_eq!(client.put(format!("{base}/api/v1/projects/{id}")).header("Authorization",&bearer).send_json(archive).unwrap().status().as_u16(),200);
        let mut response=client.get(format!("{base}/api/v1/projects")).header("Authorization",&bearer).call().unwrap();
        assert_eq!(response.body_mut().read_json::<Value>().unwrap()["items"].as_array().unwrap().len(),2);
        let mut response=client.get(format!("{base}/api/v1/projects?include_archived=true")).header("Authorization",&bearer).call().unwrap();
        assert_eq!(response.body_mut().read_json::<Value>().unwrap()["items"].as_array().unwrap().len(),3);
    }).await.unwrap();
    server.abort();
    assert!(server.await.is_err_and(|e| e.is_cancelled()));
}
