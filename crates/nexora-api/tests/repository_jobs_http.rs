use nexora_api::{ApiServer, ApiState};
use nexora_platform::{GitProvider, RepositoryRegistration};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires local PostgreSQL; authenticated real HTTP job lifecycle"]
async fn http_accepts_durable_jobs_and_replays_without_reactivating_cancelled_work() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_job_http_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let c = config.clone();
    let n = namespace.clone();
    let (token, project, other, repository) = tokio::task::spawn_blocking(move || {
        let mut store = PlatformStore::connect(&c, &n).unwrap();
        store.initialize().unwrap();
        let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
        store.bootstrap("Job HTTP fixture", "owner", &password, "Owner").unwrap();
        let session = store.login("owner", &password).unwrap();
        let ctx = store.authenticate(&session.access_token).unwrap();
        let project = store.create_project(&ctx, "Source", None).unwrap();
        let other = store.create_project(&ctx, "Other", None).unwrap();
        let repo = store.register_repository(&ctx, &project.id, &RepositoryRegistration { provider: GitProvider::Gitlab, full_name: "fixture/source".into() }).unwrap();
        // Seed authority only inside this generated fixture; no source code or provider secrets.
        let pg_password = Zeroizing::new(std::fs::read_to_string(c.secrets_directory.join("postgres_password")).unwrap());
        let mut client = postgres::Config::new().host(&c.host).port(c.postgres_port).user(&c.postgres_user).password(pg_password.trim()).dbname(&c.postgres_database).connect(postgres::NoTls).unwrap();
        client.batch_execute(&format!("SET search_path TO \"{n}\"" )).unwrap();
        let connection = SecretGenerator::id("conn").unwrap();
        client.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab','gitlab.com',$2,'42','fixture','active',1,'{}',now()+interval '1 hour')", &[&connection,&ctx.actor_id()]).unwrap();
        client.execute("UPDATE project_repositories SET remote_id='101',remote_full_name='fixture/source',connection_id=$1,authorization_status='authorized',revision=2 WHERE id=$2", &[&connection,&repo.id]).unwrap();
        (session.access_token, project.id, other.id, repo.id)
    }).await.unwrap();
    let state = ApiState::new(config, namespace, 4).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
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
        let bearer = format!("Bearer {token}");
        let sync = format!("{base}/api/v1/projects/{project}/repositories/{repository}/sync");
        let input = json!({"expected_revision":2});
        let mut response = client
            .post(&sync)
            .header("Authorization", &bearer)
            .header("Idempotency-Key", "enqueue-http-once")
            .send_json(&input)
            .unwrap();
        assert_eq!(
            response.status().as_u16(),
            202,
            "Sync route must accept a durable job"
        );
        let job: Value = response.body_mut().read_json().unwrap();
        assert_eq!(job["state"], "queued");
        assert_eq!(job["kind"], "sync");
        assert_eq!(job["attempt"], 0);
        for internal in [
            "credential_id",
            "credential_method",
            "connection_id",
            "lease_token_digest",
            "lease_owner",
            "lease_until",
        ] {
            assert!(
                job.get(internal).is_none(),
                "Internal authority leaked in HTTP job response"
            );
        }
        let id = job["id"].as_str().unwrap();
        let job_url = format!("{base}/api/v1/projects/{project}/jobs/{id}");
        let cancel_url = format!("{job_url}/cancel");
        assert_eq!(client.get(&job_url).call().unwrap().status().as_u16(), 401);
        assert_eq!(
            client
                .get(format!("{base}/api/v1/projects/{other}/jobs/{id}"))
                .header("Authorization", &bearer)
                .call()
                .unwrap()
                .status()
                .as_u16(),
            404
        );
        let mut loaded = client
            .get(&job_url)
            .header("Authorization", &bearer)
            .call()
            .unwrap();
        assert_eq!(loaded.status().as_u16(), 200);
        assert_eq!(loaded.body_mut().read_json::<Value>().unwrap(), job);
        for body in [
            json!({"expected_revision":0}),
            json!({"expected_revision":2,"root":"/tmp/source"}),
            json!({"expected_revision":2,"actor_id":"other"}),
        ] {
            assert_eq!(
                client
                    .post(&sync)
                    .header("Authorization", &bearer)
                    .send_json(body)
                    .unwrap()
                    .status()
                    .as_u16(),
                422
            );
        }
        assert_eq!(
            client
                .post(&sync)
                .header("Authorization", &bearer)
                .send_json(json!({"expected_revision":1}))
                .unwrap()
                .status()
                .as_u16(),
            409
        );
        let change = json!({"expected_revision":1});
        let mut cancelled = client
            .post(&cancel_url)
            .header("Authorization", &bearer)
            .header("Idempotency-Key", "cancel-http-once")
            .send_json(&change)
            .unwrap();
        assert_eq!(cancelled.status().as_u16(), 200);
        let cancelled: Value = cancelled.body_mut().read_json().unwrap();
        assert_eq!(cancelled["state"], "cancelled");
        let mut repeat = client
            .post(&cancel_url)
            .header("Authorization", &bearer)
            .header("Idempotency-Key", "cancel-http-once")
            .send_json(&change)
            .unwrap();
        assert_eq!(repeat.status().as_u16(), 200);
        assert_eq!(repeat.body_mut().read_json::<Value>().unwrap(), cancelled);
        assert_eq!(
            client
                .post(&cancel_url)
                .header("Authorization", &bearer)
                .send_json(&change)
                .unwrap()
                .status()
                .as_u16(),
            409
        );
        let mut historical = client
            .post(&sync)
            .header("Authorization", &bearer)
            .header("Idempotency-Key", "enqueue-http-once")
            .send_json(&input)
            .unwrap();
        assert_eq!(historical.status().as_u16(), 202);
        assert_eq!(historical.body_mut().read_json::<Value>().unwrap(), job);
        let mut current = client
            .get(&job_url)
            .header("Authorization", &bearer)
            .call()
            .unwrap();
        assert_eq!(
            current.body_mut().read_json::<Value>().unwrap()["state"],
            "cancelled"
        );
        let mut index = client
            .post(format!(
                "{base}/api/v1/projects/{project}/repositories/{repository}/index"
            ))
            .header("Authorization", &bearer)
            .send_json(input)
            .unwrap();
        assert_eq!(index.status().as_u16(), 202);
        assert_eq!(
            index.body_mut().read_json::<Value>().unwrap()["kind"],
            "index"
        );
    })
    .await
    .unwrap();
    server.abort();
    let _ = server.await;
}
