#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "../../nexora-source/tests/support/git_fixture.rs"]
mod git_fixture;
#[path = "../../nexora-platform-store/tests/support/gitlab_source_fixture.rs"]
mod gitlab_source_fixture;
#[path = "../../nexora-source/tests/support/test_directory.rs"]
mod test_directory;
#[path = "support/webhook_test_signer.rs"]
mod webhook_test_signer;
use git_fixture::GitFixture;
use gitlab_source_fixture::GitlabSourceFixture;
use nexora_api::{RepositoryWorker, RepositoryWorkerSettings};
use nexora_core::{RepositoryId, SnapshotId, TenantId};
use nexora_platform::{
    GitProvider, GitlabClientRegistration, GitlabWebhookRegistration, RepositoryJobState,
    RepositoryRegistration,
};
use nexora_platform_store::{
    GitNetworkPolicy, PlatformStore, ProviderSecretContext, SecretGenerator,
};
use nexora_runtime::{DatabaseConnections, DatabaseRuntime};
use serde_json::json;
use std::{
    os::unix::fs::DirBuilderExt,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use webhook_test_signer::WebhookTestSigner;
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local databases; provider proof through encrypted credential, Git fetch, exact source and scoped publication"]
fn signed_webhook_automatically_fetches_analyzes_and_publishes_exact_commit_without_mcp() {
    let git = GitFixture::new("job-source-plan");
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
    let provider = GitlabSourceFixture::start(&git, &commit);
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let namespace = format!("nexora_platform_worker_{nonce}");
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Source plan fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let policy = GitNetworkPolicy {
        gitlab_origin: provider.origin().into(),
        ..GitNetworkPolicy::default()
    };
    let client_secret = Zeroizing::new(SecretGenerator::token("fixture_client_secret").unwrap());
    store
        .configure_gitlab(
            &ctx,
            &GitlabClientRegistration {
                client_id: "fixture-client".into(),
                client_secret: client_secret.to_string(),
                redirect_uri: format!("{}/api/v1/git/gitlab/callback", policy.callback_origin),
                enabled: true,
                expected_revision: 0,
            },
            &policy,
        )
        .unwrap();
    let project = store.create_project(&ctx, "Source", None).unwrap();
    let repository = store
        .register_repository(
            &ctx,
            &project.id,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "fixture/source".into(),
            },
        )
        .unwrap();
    let connection = SecretGenerator::id("conn").unwrap();
    let host = policy.host().unwrap();
    let tokens = Zeroizing::new(
        serde_json::to_vec(&json!({
            "access_token":provider.token(),"refresh_token":null,"token_type":"Bearer",
            "scope":"read_api read_repository","expires_in":3600
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
                    provider: GitProvider::Gitlab.into(),
                    host: host.clone(),
                    purpose: "gitlab_tokens".into(),
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
    sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab',$2,$3,'42','fixture','active',1,$4,now()+interval '1 hour')", &[&connection,&host,&ctx.actor_id(),&encrypted]).unwrap();
    sql.execute("UPDATE project_repositories SET host=$3,remote_id='101',remote_full_name='fixture/source',connection_id=$1,authorization_status='authorized',revision=2 WHERE id=$2", &[&connection,&repository.id,&host]).unwrap();
    let signer = WebhookTestSigner::new();
    let hook = store
        .register_gitlab_webhook(
            &ctx,
            &project.id,
            &repository.id,
            &GitlabWebhookRegistration {
                signing_token: signer.token(),
                expected_revision: 2,
            },
            &policy,
        )
        .unwrap();
    let body = serde_json::to_vec(
        &json!({"project_id":101,"object_kind":"push","ref":"refs/heads/main","after":commit}),
    )
    .unwrap();
    let proof = signer.proof("automatic-analysis", &body);
    assert!(
        !store
            .receive_gitlab_webhook(&hook.id, &proof, &body, &policy)
            .unwrap()
            .duplicate
    );
    assert!(
        store
            .receive_gitlab_webhook(&hook.id, &proof, &body, &policy)
            .unwrap()
            .duplicate
    );
    assert_eq!(
        sql.query_one("SELECT count(*) FROM repository_analysis_requests", &[])
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    sql.execute(
        "UPDATE sessions SET revoked_at=clock_timestamp() WHERE id=$1",
        &[&ctx.credential_id()],
    )
    .unwrap();
    let managed = git.directory.path().canonicalize().unwrap().join("managed");
    let mut directory = std::fs::DirBuilder::new();
    directory.mode(0o700).create(&managed).unwrap();
    let language_directory = Path::new("../../languages/javascript")
        .canonicalize()
        .unwrap();
    let settings: RepositoryWorkerSettings = serde_json::from_value(json!({
        "id":"fixture-worker", "source_root":managed, "git_binary":"/usr/bin/git",
        "profiles":{(repository.id.clone()):{"directory":language_directory,"language_id":"javascript"}}
    })).unwrap();
    let mut worker =
        RepositoryWorker::connect(&config, &namespace, settings, policy.clone()).unwrap();
    let completed = worker.run_once().unwrap().unwrap();
    let origin = sql
        .query_one(
            "SELECT credential_method,source_event_id FROM repository_jobs WHERE id=$1",
            &[&completed.id],
        )
        .unwrap();
    assert_eq!(origin.get::<_, String>(0), "webhook");
    assert!(!origin.get::<_, String>(1).is_empty());
    assert_eq!(completed.state, RepositoryJobState::Succeeded);
    assert_eq!(completed.pinned_commit.as_deref(), Some(commit.as_str()));
    assert!(worker.run_once().unwrap().is_none());
    let row = sql.query_one("SELECT runtime_tenant_id,runtime_repository_id,snapshot_id FROM repository_bindings WHERE repository_id=$1", &[&repository.id]).unwrap();
    let tenant = TenantId(row.get(0));
    let runtime_repository = RepositoryId(row.get(1));
    let snapshot = SnapshotId(row.get(2));
    assert_eq!(completed.snapshot_id.as_deref(), Some(snapshot.0.as_str()));
    let mut runtime = DatabaseRuntime::connect(&config).unwrap();
    assert!(
        runtime
            .snapshot(&tenant, &runtime_repository, Some(&snapshot))
            .unwrap()
            .is_some()
    );
    assert!(
        runtime
            .snapshot(&tenant, &runtime_repository, None)
            .unwrap()
            .is_none()
    );
    assert_eq!(std::fs::read_dir(&managed).unwrap().count(), 0);
    let before: String = sql
        .query_one(
            "SELECT job_id FROM repository_bindings WHERE repository_id=$1",
            &[&repository.id],
        )
        .unwrap()
        .get(0);
    provider.control(json!({"deny_repository":true}));
    let proof = signer.proof("automatic-provider-denial", &body);
    store
        .receive_gitlab_webhook(&hook.id, &proof, &body, &policy)
        .unwrap();
    let denied = worker.run_once().unwrap().unwrap();
    assert_eq!(denied.state, RepositoryJobState::Failed);
    assert_eq!(denied.error_code.as_deref(), Some("source_access_denied"));
    assert_eq!(
        sql.query_one(
            "SELECT job_id FROM repository_bindings WHERE repository_id=$1",
            &[&repository.id]
        )
        .unwrap()
        .get::<_, String>(0),
        before
    );
}
