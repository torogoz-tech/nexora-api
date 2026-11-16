#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "../../nexora-source/tests/support/git_fixture.rs"]
mod git_fixture;
#[path = "../../nexora-platform-store/tests/support/gitlab_source_fixture.rs"]
mod gitlab_source_fixture;
#[path = "../../nexora-source/tests/support/test_directory.rs"]
mod test_directory;
use git_fixture::GitFixture;
use gitlab_source_fixture::GitlabSourceFixture;
use nexora_api::{RepositoryWorker, RepositoryWorkerSettings};
use nexora_core::{RepositoryId, SnapshotId, TenantId};
use nexora_platform::{
    GitProvider, GitlabClientRegistration, RepositoryJobKind, RepositoryJobRequest,
    RepositoryJobState, RepositoryRegistration,
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
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local databases; provider proof through encrypted credential, Git fetch, exact source and scoped publication"]
fn worker_processes_durable_jobs_and_preserves_publication_on_source_failure() {
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
    store
        .change_repository_analysis_profile(
            &ctx,
            &project.id,
            &repository.id,
            &nexora_platform::RepositoryAnalysisProfileChange {
                expected_revision: 0,
                profile_version_id: None,
                mode: None,
                profile_version_ids: None,
                reason: "Legacy worker fixture".into(),
            },
        )
        .unwrap();
    let input = RepositoryJobRequest {
        expected_revision: 2,
    };
    let job = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Index,
            &input,
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
    assert_eq!(completed.id, job.id);
    assert_eq!(completed.state, RepositoryJobState::Succeeded);
    assert_eq!(completed.pinned_commit.as_deref(), Some(commit.as_str()));
    assert!(worker.run_once().unwrap().is_none());
    let row = sql.query_one("SELECT runtime_tenant_id,runtime_repository_id,snapshot_id FROM repository_bindings WHERE repository_id=$1", &[&repository.id]).unwrap();
    let tenant = TenantId(row.get(0));
    let runtime_repository = RepositoryId(row.get(1));
    let mut snapshot = SnapshotId(row.get(2));
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
    // Imported models work with an empty operator profile map; queued work pins its version.
    let package = nexora_language::AnalysisProfilePackage::from_catalog(
        Path::new("../../languages/javascript"),
        "javascript".into(),
        "worker-javascript".into(),
        "1.0.0".into(),
        "Worker JavaScript".into(),
    )
    .unwrap();
    let profile = store.import_analysis_profile(&ctx, &package).unwrap();
    let selected = store
        .change_repository_analysis_profile(
            &ctx,
            &project.id,
            &repository.id,
            &nexora_platform::RepositoryAnalysisProfileChange {
                expected_revision: 1,
                mode: None,
                profile_version_ids: None,
                profile_version_id: profile["id"].as_str().map(str::to_owned),
                reason: "Managed fixture".into(),
            },
        )
        .unwrap();
    let managed_job = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Index,
            &input,
        )
        .unwrap();
    assert_eq!(
        managed_job.analysis_profile_version_id.as_deref(),
        profile["id"].as_str()
    );
    // Clearing the current assignment must not change this already-enqueued job.
    store
        .change_repository_analysis_profile(
            &ctx,
            &project.id,
            &repository.id,
            &nexora_platform::RepositoryAnalysisProfileChange {
                expected_revision: selected["revision"].as_i64().unwrap(),
                mode: None,
                profile_version_ids: None,
                profile_version_id: None,
                reason: "Verify pinned work".into(),
            },
        )
        .unwrap();
    let managed_settings=serde_json::from_value(json!({"id":"managed-profile-worker","source_root":managed,"git_binary":"/usr/bin/git","profiles":{}})).unwrap();
    let mut managed_worker =
        RepositoryWorker::connect(&config, &namespace, managed_settings, policy.clone()).unwrap();
    let managed_result = managed_worker.run_once().unwrap().unwrap();
    assert_eq!(managed_result.id, managed_job.id);
    assert_eq!(managed_result.state, RepositoryJobState::Succeeded);
    snapshot = SnapshotId(managed_result.snapshot_id.clone().unwrap());
    // A real graph storage outage during staging must not be classified as
    // invalid source/analysis, and must preserve the last published binding.
    let storage_job = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Index,
            &input,
        )
        .unwrap();
    let unavailable = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut outage_config = config.clone();
    outage_config.neo4j_http_port = unavailable.local_addr().unwrap().port();
    drop(unavailable);
    let outage_settings = serde_json::from_value(json!({
        "id":"storage-outage-worker", "source_root":managed,
        "git_binary":"/usr/bin/git", "profiles":{(repository.id.clone()):{
            "directory":language_directory,"language_id":"javascript"}}
    }))
    .unwrap();
    let mut outage_worker =
        RepositoryWorker::connect(&outage_config, &namespace, outage_settings, policy.clone())
            .unwrap();
    let failed_storage = outage_worker.run_once().unwrap().unwrap();
    assert_eq!(failed_storage.id, storage_job.id);
    assert_eq!(
        failed_storage.error_code.as_deref(),
        Some("storage_unavailable")
    );
    // Manual requests retain their existing terminal/retry contract.
    assert_eq!(failed_storage.state, RepositoryJobState::Failed);
    assert_eq!(
        sql.query_one(
            "SELECT snapshot_id FROM repository_bindings WHERE repository_id=$1",
            &[&repository.id]
        )
        .unwrap()
        .get::<_, String>(0),
        snapshot.0
    );
    let recovery_job = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Index,
            &input,
        )
        .unwrap();
    let recovered = worker.run_once().unwrap().unwrap();
    assert_eq!(recovered.id, recovery_job.id);
    assert_eq!(recovered.state, RepositoryJobState::Succeeded);
    snapshot = SnapshotId(recovered.snapshot_id.clone().unwrap());
    // A missing operator profile must fail before contacting or revoking the source.
    let missing_profile = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &input,
        )
        .unwrap();
    let settings = serde_json::from_value(json!({
        "id":"profileless-worker", "source_root":managed,
        "git_binary":"/usr/bin/git", "profiles":{}
    }))
    .unwrap();
    let mut profileless =
        RepositoryWorker::connect(&config, &namespace, settings, policy.clone()).unwrap();
    provider.control(json!({"deny_repository":true}));
    let missing = profileless.run_once().unwrap().unwrap();
    assert_eq!(missing.id, missing_profile.id);
    assert_eq!(missing.error_code.as_deref(), Some("profile_unavailable"));
    assert_eq!(
        sql.query_one(
            "SELECT authorization_status FROM project_repositories WHERE id=$1",
            &[&repository.id]
        )
        .unwrap()
        .get::<_, String>(0),
        "authorized"
    );
    drop(profileless);
    let rejected = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &input,
        )
        .unwrap();
    provider.control(json!({"deny_repository":true}));
    let failed = worker.run_once().unwrap().unwrap();
    assert_eq!(failed.id, rejected.id);
    assert_eq!(failed.state, RepositoryJobState::Failed);
    assert_eq!(failed.error_code.as_deref(), Some("source_access_denied"));
    assert_eq!(
        sql.query_one(
            "SELECT snapshot_id FROM repository_bindings WHERE repository_id=$1",
            &[&repository.id]
        )
        .unwrap()
        .get::<_, String>(0),
        snapshot.0
    );
    assert_eq!(std::fs::read_dir(&managed).unwrap().count(), 0);
    assert!(worker.run_once().unwrap().is_none());
    let settings_path = git.directory.path().join("worker.json");
    std::fs::write(
        &settings_path,
        serde_json::to_vec(&json!({
            "id":"cli-worker", "source_root":managed,
            "git_binary":"/usr/bin/git", "profiles":{}
        }))
        .unwrap(),
    )
    .unwrap();
    let policy_path = git.directory.path().join("git-policy.json");
    std::fs::write(&policy_path, serde_json::to_vec(&policy).unwrap()).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nexora-api"))
        .args([
            "--connections",
            "../../.nexora/local/connections.json",
            "--schema",
            &namespace,
            "worker",
            "--settings",
        ])
        .arg(&settings_path)
        .arg("--git-policy")
        .arg(&policy_path)
        .arg("--once")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "worker command failed; output suppressed"
    );
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response, json!({"job":null}));
}
