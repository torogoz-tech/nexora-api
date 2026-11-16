#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "../../nexora-source/tests/support/git_fixture.rs"]
mod git_fixture;
#[path = "../../nexora-platform-store/tests/support/github_source_fixture.rs"]
mod github_source_fixture;
#[path = "../../nexora-source/tests/support/test_directory.rs"]
mod test_directory;
use git_fixture::GitFixture;
use github_source_fixture::GithubSourceFixture;
use hmac::{Hmac, Mac};
use nexora_api::{RepositoryWorker, RepositoryWorkerSettings};
use nexora_platform::{GitProvider, RepositoryJobState, RepositoryRegistration};
use nexora_platform_store::{
    GitNetworkPolicy, PlatformStore, ProviderSecretContext, SecretGenerator, WebhookProof,
};
use nexora_runtime::{DatabaseConnections, DatabaseRuntime};
use serde_json::json;
use sha2::Sha256;
use std::{
    os::unix::fs::DirBuilderExt,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local databases; provider proof through encrypted credential, Git fetch, exact source and scoped publication"]
fn github_push_queues_exact_commit_and_worker_publishes_without_mcp_or_live_developer_login() {
    let git = GitFixture::new("github-job-source-plan");
    let code = b"export function generated() { return 7; }\n";
    let blob = git.blob(code);
    let attributes = git.blob(b"source.js export-ignore\n");
    let tree = git.tree(&[
        ("100644", &attributes, ".gitattributes"),
        ("100644", &blob, "source.js"),
    ]);
    let commit = git.commit(&tree);
    git.run(&["update-ref", "refs/heads/main", &commit], b"");
    git.run(&["symbolic-ref", "HEAD", "refs/heads/main"], b"");
    let provider = GithubSourceFixture::start(&git, &commit);
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let namespace = format!("nexora_platform_gh_source_{nonce}");
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Source plan fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let verified = store
        .identity_gate(&session.access_token, "skip", "")
        .unwrap();
    let ctx = store
        .authenticate(verified["access_token"].as_str().unwrap())
        .unwrap();
    let policy = GitNetworkPolicy {
        github_origin: provider.origin().into(),
        github_api_origin: provider.origin().into(),
        ..GitNetworkPolicy::default()
    };
    let registration = provider.registration(&policy.callback_origin);
    let webhook_secret = Zeroizing::new(registration.webhook_secret.clone());
    store
        .configure_github(&ctx, &registration, &policy)
        .unwrap();
    let project = store.create_project(&ctx, "Source", None).unwrap();
    let repository = store
        .register_repository(
            &ctx,
            &project.id,
            &RepositoryRegistration {
                provider: GitProvider::Github,
                full_name: "fixture/source".into(),
            },
        )
        .unwrap();
    store
        .change_repository_analysis_profile(
            &ctx,
            &project.id,
            &repository.id,
            &nexora_platform::RepositoryAnalysisProfileChange {
                expected_revision: 0,
                profile_version_id: None,
                reason: "Legacy worker fixture".into(),
                mode: None,
                profile_version_ids: None,
            },
        )
        .unwrap();
    let connection = SecretGenerator::id("conn").unwrap();
    let host = policy.github_host().unwrap();
    let tokens = Zeroizing::new(
        serde_json::to_vec(&json!({
            "access_token":provider.token(),"refresh_token":SecretGenerator::token("fixture_refresh").unwrap(),"token_type":"Bearer",
            "scope":"","expires_in":3600,"refresh_token_expires_in":86400
        }))
        .unwrap(),
    );
    let encrypted = serde_json::to_value(
        store
            .provider_secret_cipher()
            .unwrap()
            .seal(
                &ProviderSecretContext {
                    binding_id: connection.clone(),
                    provider: GitProvider::Github.into(),
                    host: host.clone(),
                    purpose: "github_user_tokens".into(),
                    revision: 1,
                },
                &tokens,
            )
            .unwrap(),
    )
    .unwrap();
    let pg_password = Zeroizing::new(
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(pg_password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{namespace}\""))
        .unwrap();
    sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at,installation_id) VALUES($1,'github',$2,$3,'42','fixture','active',1,$4,now()+interval '1 hour','7')", &[&connection,&host,&ctx.actor_id(),&encrypted]).unwrap();
    sql.execute("UPDATE project_repositories SET host=$3,remote_id='101',remote_full_name='fixture/source',connection_id=$1,authorization_status='authorized',revision=2 WHERE id=$2", &[&connection,&repository.id,&host]).unwrap();
    let body=serde_json::to_vec(&json!({"ref":"refs/heads/main","after":commit,"repository":{"id":101},"installation":{"id":7}})).unwrap();
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(webhook_secret.as_bytes()).unwrap();
    mac.update(&body);
    let signature = format!(
        "sha256={}",
        mac.finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let proof = |id: &str| WebhookProof {
        delivery_id: id.into(),
        event: "push".into(),
        signature: signature.clone(),
        timestamp: None,
    };
    store
        .receive_github_webhook(&proof("automatic-github"), &body, &policy)
        .unwrap();
    assert!(
        store
            .receive_github_webhook(&proof("automatic-github"), &body, &policy)
            .unwrap()
            .duplicate
    );
    sql.execute(
        "UPDATE sessions SET revoked_at=clock_timestamp() WHERE id=$1",
        &[&ctx.credential_id()],
    )
    .unwrap();
    let managed = git.directory.path().canonicalize().unwrap().join("managed");
    let mut directory = std::fs::DirBuilder::new();
    directory.mode(0o700).create(&managed).unwrap();
    let language = Path::new("../../languages/javascript")
        .canonicalize()
        .unwrap();
    let settings:RepositoryWorkerSettings=serde_json::from_value(json!({"id":"github-automatic-worker","source_root":managed,"git_binary":"/usr/bin/git","profiles":{(repository.id.clone()):{"directory":language,"language_id":"javascript"}}})).unwrap();
    let mut worker =
        RepositoryWorker::connect(&config, &namespace, settings, policy.clone()).unwrap();
    // A malformed default-head response cannot replace the webhook's pinned commit.
    provider.control(json!({"invalid_commit":true}));
    // Deterministic race: configuration owns settings before updating connections.
    let mut observer = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(pg_password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    let backend: i32 = sql
        .query_one("SELECT pg_backend_pid()", &[])
        .unwrap()
        .get(0);
    let mut rotation = sql.transaction().unwrap();
    rotation
        .batch_execute("SET LOCAL lock_timeout='2s'")
        .unwrap();
    rotation
        .query_one(
            "SELECT app_id FROM github_settings WHERE singleton FOR UPDATE",
            &[],
        )
        .unwrap();
    let attempt = std::thread::spawn(move || {
        let outcome = worker.run_once();
        (worker, outcome)
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let blocked: bool = observer
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
                &[&backend],
            )
            .unwrap()
            .get(0);
        if blocked {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "worker did not reach the blocked settings read"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    rotation
        .execute(
            "UPDATE git_connections SET updated_at=updated_at WHERE id=$1",
            &[&connection],
        )
        .unwrap();
    rotation.commit().unwrap();
    let (mut worker, result) = attempt.join().unwrap();
    let result = result.unwrap().unwrap();

    assert_eq!(result.state, RepositoryJobState::Succeeded);
    assert_eq!(result.pinned_commit.as_deref(), Some(commit.as_str()));
    let origin = sql
        .query_one(
            "SELECT credential_method,source_event_id FROM repository_jobs WHERE id=$1",
            &[&result.id],
        )
        .unwrap();
    assert_eq!(origin.get::<_, String>(0), "webhook");
    assert!(!origin.get::<_, String>(1).is_empty());
    let binding=sql.query_one("SELECT runtime_tenant_id,runtime_repository_id,snapshot_id FROM repository_bindings WHERE repository_id=$1",&[&repository.id]).unwrap();
    let mut runtime = DatabaseRuntime::connect(&config).unwrap();
    assert!(
        runtime
            .snapshot(
                &nexora_core::TenantId(binding.get(0)),
                &nexora_core::RepositoryId(binding.get(1)),
                Some(&nexora_core::SnapshotId(binding.get(2)))
            )
            .unwrap()
            .is_some()
    );
    provider.control(json!({"rate_limit":true}));
    store
        .receive_github_webhook(&proof("automatic-github-retry"), &body, &policy)
        .unwrap();
    let retry = worker.run_once().unwrap().unwrap();
    assert_eq!(retry.state, RepositoryJobState::Queued);
    assert_eq!(retry.error_code.as_deref(), Some("source_unavailable"));
    sql.execute(
        "UPDATE repository_jobs SET available_at=clock_timestamp() WHERE id=$1",
        &[&retry.id],
    )
    .unwrap();
    provider.control(json!({}));
    let result = worker.run_once().unwrap().unwrap();
    assert_eq!(result.id, retry.id);
    assert_eq!(result.state, RepositoryJobState::Succeeded);
}
