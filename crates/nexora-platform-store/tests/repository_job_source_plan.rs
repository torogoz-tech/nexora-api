#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "../../nexora-source/tests/support/git_fixture.rs"]
mod git_fixture;
#[path = "support/gitlab_source_fixture.rs"]
mod gitlab_source_fixture;
#[path = "../../nexora-source/tests/support/test_directory.rs"]
mod test_directory;
use git_fixture::GitFixture;
use gitlab_source_fixture::GitlabSourceFixture;
use nexora_core::{RepositoryId, TenantId};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use nexora_platform::{
    GitProvider, GitlabClientRegistration, PlatformError, RepositoryJobKind, RepositoryJobRequest,
    RepositoryJobState, RepositoryRegistration,
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
fn source_plan_connects_verified_provider_commit_to_materialization_and_publication() {
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
    let namespace = format!("nexora_platform_job_source_{nonce}");
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

    // A definitive denial concerns this repository; it must not revoke the entire connection.
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
    provider.control(json!({"deny_repository":true}));
    assert!(matches!(
        store.prepare_repository_job_source(&denied, &policy),
        Err(PlatformError::Forbidden)
    ));
    assert_eq!(
        sql.query_one(
            "SELECT authorization_status FROM project_repositories WHERE id=$1",
            &[&repository.id]
        )
        .unwrap()
        .get::<_, String>(0),
        "revoked"
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
}
