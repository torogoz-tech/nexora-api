#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "support/git_fixture.rs"]
mod git_fixture;
#[path = "support/github_source_fixture.rs"]
mod github_source_fixture;
#[path = "support/test_directory.rs"]
mod test_directory;
use git_fixture::GitFixture;
use github_source_fixture::GithubSourceFixture;
use nexora_core::{RepositoryId, TenantId};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use nexora_platform::{
    GitProvider, PlatformError, RepositoryJobKind, RepositoryJobRequest, RepositoryJobState,
    RepositoryRegistration,
};
use nexora_platform_store::{
    GitNetworkPolicy, PlatformStore, ProviderSecretContext, SecretGenerator,
};
use nexora_runtime::{DatabaseConnections, DatabaseRuntime};
use nexora_source::{
    GitFetchPolicy, GitFetcher, GitMaterializer, GitObjectReader, GitTransferLimits, SourceLimits,
};
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local databases; provider proof through encrypted credential, Git fetch, exact source and scoped publication"]
fn github_source_plan_uses_repository_scoped_token_and_publishes_the_verified_commit() {
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
    let other_password = Zeroizing::new(SecretGenerator::token("fixture_other").unwrap());
    store
        .create_user(&ctx, "other", &other_password, "Other")
        .unwrap();
    let other_session = store.login("other", &other_password).unwrap();
    let other_verified = store
        .identity_gate(&other_session.access_token, "skip", "")
        .unwrap();
    let other = store
        .authenticate(other_verified["access_token"].as_str().unwrap())
        .unwrap();
    assert!(matches!(
        store.github_webhook_health(&other),
        Err(PlatformError::Forbidden)
    ));
    let policy = GitNetworkPolicy {
        github_origin: provider.origin().into(),
        github_api_origin: provider.origin().into(),
        ..GitNetworkPolicy::default()
    };
    store
        .configure_github(
            &ctx,
            &provider.registration(&policy.callback_origin),
            &policy,
        )
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
    // This fixture exercises the legacy single-indexer publication path explicitly.
    store
        .change_repository_analysis_profile(
            &ctx,
            &project.id,
            &repository.id,
            &nexora_platform::RepositoryAnalysisProfileChange {
                expected_revision: 0,
                profile_version_id: None,
                reason: "Legacy publication fixture".into(),
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
    let lease = store
        .claim_repository_job("source-plan", 300)
        .unwrap()
        .unwrap();
    provider.control(json!({"rate_limit":true}));
    let limited_error = store.prepare_repository_job_source(&lease, &policy).err();
    assert!(
        matches!(limited_error, Some(PlatformError::Unavailable)),
        "Rate-limited source plan returned {limited_error:?}"
    );
    assert!(
        store
            .repository_job(&ctx, &project.id, &job.id)
            .unwrap()
            .pinned_commit
            .is_none()
    );
    assert_eq!(
        sql.query_one(
            "SELECT authorization_status FROM project_repositories WHERE id=$1",
            &[&repository.id]
        )
        .unwrap()
        .get::<_, String>(0),
        "authorized"
    );
    provider.control(json!({"invalid_commit":true}));
    assert!(matches!(
        store.prepare_repository_job_source(&lease, &policy),
        Err(PlatformError::Unavailable)
    ));
    assert!(
        store
            .repository_job(&ctx, &project.id, &job.id)
            .unwrap()
            .pinned_commit
            .is_none()
    );
    provider.control(json!({}));
    let plan = store
        .prepare_repository_job_source(&lease, &policy)
        .unwrap();
    assert_eq!(plan.origin(), provider.origin());
    assert_eq!(plan.repository(), "fixture/source");
    assert_eq!(plan.commit(), commit);
    assert_eq!(plan.scope().repository_id, repository.id);
    assert_eq!(
        store
            .repository_job(&ctx, &project.id, &job.id)
            .unwrap()
            .pinned_commit
            .as_deref(),
        Some(commit.as_str())
    );
    // A pinned retry must read the fixed commit, even when the default-head reply is malformed.
    provider.control(json!({"invalid_commit":true}));
    let pinned = store
        .prepare_repository_job_source(&lease, &policy)
        .unwrap();
    assert_eq!(pinned.commit(), commit);
    provider.control(json!({}));
    // The import checkpoints each page, resumes safely and never invents pushes.
    provider.control(json!({"fail_page_two":true}));
    let failure = store.import_repository_commits(&lease, &policy);
    assert!(
        matches!(failure, Err(PlatformError::Unavailable)),
        "{failure:?}"
    );
    assert_eq!(
        sql.query_one(
            "SELECT next_page FROM repository_commit_imports WHERE repository_id=$1",
            &[&repository.id]
        )
        .unwrap()
        .get::<_, i64>(0),
        2
    );
    assert_eq!(
        sql.query_one("SELECT count(*) FROM git_commit_authorship", &[])
            .unwrap()
            .get::<_, i64>(0),
        20
    );
    provider.control(json!({}));
    store.import_repository_commits(&lease, &policy).unwrap();
    store.import_repository_commits(&lease, &policy).unwrap();
    assert_eq!(
        sql.query_one(
            "SELECT count(*) FROM git_commit_authorship WHERE occurred_at='2026-10-01T12:00:00Z'",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        21
    );
    assert_eq!(
        sql.query_one("SELECT count(*) FROM developer_push_activity", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    let health = store.github_webhook_health(&ctx).unwrap();
    assert_eq!(health["status"]["configuration_verified"], true);
    assert_eq!(health["status"]["push_subscribed"], true);
    assert!(!health.to_string().contains("********"));
    provider.control(json!({"no_push":true}));
    assert_eq!(
        store.check_github_webhook(&ctx, &policy).unwrap()["push_subscribed"],
        false
    );
    provider.control(json!({"hook_failure":true}));
    assert!(store.check_github_webhook(&ctx, &policy).is_err());
    assert_eq!(
        store.github_webhook_health(&ctx).unwrap()["status"]["configuration_verified"],
        false
    );
    // Incremental heads use compare; rewritten bases recover with a full import.
    sql.execute("UPDATE repository_commit_imports SET target_commit=$2,complete=true WHERE repository_id=$1",&[&repository.id,&"f".repeat(40)]).unwrap();
    provider.control(json!({}));
    store.import_repository_commits(&lease, &policy).unwrap();
    assert_eq!(
        sql.query_one("SELECT count(*) FROM git_commit_authorship", &[])
            .unwrap()
            .get::<_, i64>(0),
        22
    );
    sql.execute("UPDATE repository_commit_imports SET target_commit=$2,complete=true WHERE repository_id=$1",&[&repository.id,&"e".repeat(40)]).unwrap();
    provider.control(json!({"compare_missing":true}));
    store.import_repository_commits(&lease, &policy).unwrap();
    assert_eq!(
        sql.query_one("SELECT count(*) FROM git_commit_authorship", &[])
            .unwrap()
            .get::<_, i64>(0),
        22
    );
    provider.control(json!({}));
    provider.control(json!({"branches":true}));
    store.import_repository_branches(&lease, &policy).unwrap();
    store.import_repository_branches(&lease, &policy).unwrap();
    assert_eq!(
        sql.query_one("SELECT count(*) FROM git_commit_authorship", &[])
            .unwrap()
            .get::<_, i64>(0),
        23
    );
    assert_eq!(
        sql.query_one(
            "SELECT count(*) FROM git_commit_branches WHERE repository_id=$1 AND commit_sha=$2",
            &[&repository.id, &plan.commit().to_string()]
        )
        .unwrap()
        .get::<_, i64>(0),
        2
    );
    assert_eq!(
        sql.query_one(
            "SELECT count(*) FROM repository_branch_imports WHERE complete",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        2
    );
    assert_eq!(
        sql.query_one(
            "SELECT count(*) FROM git_commit_authorship WHERE commit_message IS NULL",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        0
    );
    provider.control(json!({}));
    let detail = store
        .repository_commit_detail(&ctx, &project.id, &repository.id, plan.commit(), 1, &policy)
        .unwrap();
    assert_eq!(detail["files"][0]["path"], "src/example.rs");
    assert!(detail["message"].as_str().unwrap().starts_with("Commit "));
    assert_eq!(detail["has_more"], false);
    assert!(
        store
            .repository_commit_detail(&ctx, &project.id, &repository.id, "../invalid", 1, &policy)
            .is_err()
    );
    let fetcher =
        GitFetcher::new(PathBuf::from("/usr/bin/git"), GitTransferLimits::default()).unwrap();
    let fetched = fetcher
        .fetch(
            &GitFetchPolicy::new(plan.origin()).unwrap(),
            plan.repository(),
            plan.commit(),
            plan.format(),
            plan.credentials(),
            &git.directory.path().join("download"),
        )
        .unwrap();
    let reader =
        GitObjectReader::new(PathBuf::from("/usr/bin/git"), Duration::from_secs(10)).unwrap();
    let captured = git.directory.path().join("captured");
    GitMaterializer::materialize(
        &reader,
        fetched.bare(),
        plan.commit(),
        plan.format(),
        &captured,
        &SourceLimits::default(),
    )
    .unwrap();
    assert_eq!(std::fs::read(captured.join("source.js")).unwrap(), code);
    // macOS exposes its temporary directory through /var; index only its physical path.
    let captured = captured.canonicalize().unwrap();
    let tenant = TenantId(plan.scope().tenant_id.clone());
    let runtime_repository = RepositoryId(plan.scope().repository_id.clone());
    let indexer =
        Indexer::new(LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap());
    let mut runtime = DatabaseRuntime::connect(&config).unwrap();
    let result = runtime
        .stage_repository(
            &indexer,
            &captured,
            tenant.clone(),
            runtime_repository.clone(),
        )
        .unwrap();
    let header = runtime
        .query_header(&tenant, &runtime_repository, Some(&result.snapshot.id))
        .unwrap();
    let binding = store.complete_repository_job(&lease, &header).unwrap();
    assert_eq!(binding.pinned_commit, commit);
    assert_eq!(binding.payload_hash, header.payload_hash);
    assert_eq!(
        store
            .repository_job(&ctx, &project.id, &job.id)
            .unwrap()
            .state,
        RepositoryJobState::Succeeded
    );
    assert!(
        runtime
            .snapshot(&tenant, &runtime_repository, None)
            .unwrap()
            .is_none()
    );

    // An HTTP access failure denies this attempt, never changes durable consent.
    store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &input,
        )
        .unwrap();
    let denied = store
        .claim_repository_job("source-denied", 300)
        .unwrap()
        .unwrap();
    for (path, status) in [
        ("/repos/fixture/source", 403),
        ("/repos/fixture/source", 401),
        ("/repos/fixture/source", 404),
        ("/app/installations/7", 404),
        ("/app/installations/7", 403),
        ("/user", 401),
        ("/user", 503),
        ("/repos/fixture/source", 502),
    ] {
        provider.control(json!({"failure_path":path,"failure_status":status}));
        let result = store.prepare_repository_job_source(&denied, &policy);
        assert!(
            result.is_err(),
            "Access must remain denied for {path}: {status}"
        );
        assert_eq!(
            sql.query_one(
                "SELECT authorization_status FROM project_repositories WHERE id=$1",
                &[&repository.id]
            )
            .unwrap()
            .get::<_, String>(0),
            "authorized",
            "HTTP {status} from {path} must not withdraw repository consent"
        );
        assert_eq!(
            sql.query_one(
                "SELECT status FROM git_connections WHERE id=$1",
                &[&connection]
            )
            .unwrap()
            .get::<_, String>(0),
            "active",
            "HTTP {status} from {path} must not revoke the connection"
        );
    }
    // Interactive listing uses the same credentials and must also fail without revocation.
    provider.control(json!({"failure_path":"/user","failure_status":401}));
    assert!(
        store
            .connection_remote_repositories(&ctx, &connection, 1, 20, &policy)
            .is_err()
    );
    assert_eq!(
        sql.query_one(
            "SELECT status FROM git_connections WHERE id=$1",
            &[&connection]
        )
        .unwrap()
        .get::<_, String>(0),
        "active"
    );
    provider.control(json!({}));
    let recovered = store
        .prepare_repository_job_source(&denied, &policy)
        .unwrap();
    assert_eq!(recovered.commit(), commit);
    // Explicit human revocation still withdraws the connection and its repositories.
    store.revoke_git_connection(&ctx, &connection).unwrap();
    assert_eq!(
        sql.query_one(
            "SELECT status FROM git_connections WHERE id=$1",
            &[&connection]
        )
        .unwrap()
        .get::<_, String>(0),
        "revoked"
    );
    assert_eq!(
        sql.query_one(
            "SELECT authorization_status FROM project_repositories WHERE id=$1",
            &[&repository.id]
        )
        .unwrap()
        .get::<_, String>(0),
        "revoked"
    );
}
