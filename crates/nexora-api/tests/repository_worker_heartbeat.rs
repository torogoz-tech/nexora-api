#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "support/git_fetch_pause.rs"]
mod git_fetch_pause;
#[path = "../../nexora-source/tests/support/git_fixture.rs"]
mod git_fixture;
#[path = "../../nexora-platform-store/tests/support/gitlab_source_fixture.rs"]
mod gitlab_source_fixture;
#[path = "../../nexora-source/tests/support/test_directory.rs"]
mod test_directory;
use git_fetch_pause::GitFetchPause;
use git_fixture::GitFixture;
use gitlab_source_fixture::GitlabSourceFixture;
use nexora_api::RepositoryWorker;
use nexora_platform::{
    GitProvider, GitlabClientRegistration, RepositoryJobKind, RepositoryJobRequest,
    RepositoryJobState, RepositoryRegistration,
};
use nexora_platform_store::{
    GitNetworkPolicy, PlatformStore, ProviderSecretContext, SecretGenerator,
};
use nexora_runtime::DatabaseConnections;
use serde_json::json;
use std::{
    os::unix::fs::DirBuilderExt,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local databases; intentionally holds Git fetch across the real thirty-second lease supervisor interval"]
fn worker_renews_its_lease_while_authenticated_git_fetch_is_blocked() {
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
    provider.control(json!({}));
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let namespace = format!("nexora_platform_worker_heartbeat_{nonce}");
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
    let input = RepositoryJobRequest {
        expected_revision: 2,
    };
    // Keep this fixture exercising the legacy operator mapping; automatic
    // managed selection has its own mixed-repository regression.
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

    let settings = serde_json::from_value(json!({
        "id":"heartbeat-worker", "source_root":managed,"git_binary":"/usr/bin/git",
        "profiles":{(repository.id.clone()):{"directory":language_directory,"language_id":"javascript"}}
    })).unwrap();
    let mut worker = RepositoryWorker::connect(&config, &namespace, settings, policy).unwrap();
    let pause = GitFetchPause::new(&git.directory.path().join("provider-control.json"));
    let task = std::thread::spawn(move || worker.run_once());
    let observed = pause.wait();
    let mut shortened = 0;
    let mut renewed = false;
    if observed {
        // Only this fixture's running attempt is shortened; production timing is unchanged.
        shortened = sql.execute("UPDATE repository_jobs SET lease_until=clock_timestamp()+interval '45 seconds' WHERE id=$1 AND state='running'", &[&job.id]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(40);
        while Instant::now() < deadline {
            renewed = sql.query_one("SELECT state='running' AND lease_until>clock_timestamp()+interval '240 seconds' FROM repository_jobs WHERE id=$1", &[&job.id]).unwrap().get(0);
            if renewed || task.is_finished() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    pause.release();
    let result = task.join().unwrap();
    assert!(
        observed,
        "Fixture did not observe the authenticated Git fetch"
    );
    assert_eq!(shortened, 1);
    assert!(
        renewed,
        "Supervisor did not renew the lease while the fetch was blocked"
    );
    let result = result.unwrap().unwrap();
    assert_eq!(result.id, job.id);
    assert_eq!(result.state, RepositoryJobState::Succeeded);
    assert_eq!(result.pinned_commit.as_deref(), Some(commit.as_str()));
    assert_eq!(
        sql.query_one(
            "SELECT count(*) FROM repository_bindings WHERE repository_id=$1 AND job_id=$2",
            &[&repository.id, &job.id]
        )
        .unwrap()
        .get::<_, i64>(0),
        1
    );
    assert_eq!(std::fs::read_dir(&managed).unwrap().count(), 0);
}
