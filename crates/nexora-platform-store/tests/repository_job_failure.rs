use nexora_platform::{
    GitProvider, PlatformError, RepositoryJobChange, RepositoryJobFailure, RepositoryJobKind,
    RepositoryJobRequest, RepositoryJobState, RepositoryRegistration,
};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local PostgreSQL; terminal failure must be fenced and work after source revocation"]
fn worker_failure_requires_current_lease_and_never_changes_cancelled_or_reclaimed_jobs() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_job_failure_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Lease fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
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
    let pg_password = Zeroizing::new(
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut client = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(pg_password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    client
        .batch_execute(&format!("SET search_path TO \"{namespace}\""))
        .unwrap();
    let connection = SecretGenerator::id("conn").unwrap();
    client.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab','gitlab.com',$2,'42','fixture','active',1,'{}',now()+interval '1 hour')", &[&connection,&ctx.actor_id()]).unwrap();
    client.execute("UPDATE project_repositories SET remote_id='101',remote_full_name='fixture/source',connection_id=$1,authorization_status='authorized',revision=2 WHERE id=$2", &[&connection,&repository.id]).unwrap();

    let input = RepositoryJobRequest {
        expected_revision: 2,
    };
    let job = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &input,
        )
        .unwrap();
    let stale = store
        .claim_repository_job("worker-one", 60)
        .unwrap()
        .unwrap();
    client.execute("UPDATE repository_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1", &[&job.id]).unwrap();
    let current = store
        .claim_repository_job("worker-two", 60)
        .unwrap()
        .unwrap();
    assert!(matches!(
        store.fail_repository_job(&stale, RepositoryJobFailure::SourceAccessDenied),
        Err(PlatformError::Conflict(_))
    ));
    client.execute("UPDATE project_repositories SET authorization_status='revoked',revision=revision+1 WHERE id=$1", &[&repository.id]).unwrap();
    let failed = store
        .fail_repository_job(&current, RepositoryJobFailure::SourceAccessDenied)
        .unwrap();
    assert_eq!(failed.state, RepositoryJobState::Failed);
    assert_eq!(failed.error_code.as_deref(), Some("source_access_denied"));
    assert_eq!(failed.attempt, 2);
    assert!(matches!(
        store.fail_repository_job(&current, RepositoryJobFailure::SourceAccessDenied),
        Err(PlatformError::Conflict(_))
    ));
    assert_eq!(client.query_one("SELECT count(*) FROM audit_events WHERE operation='repository.job_failed' AND resource_id=$1", &[&job.id]).unwrap().get::<_,i64>(0), 1);
    client.execute("UPDATE project_repositories SET authorization_status='authorized',revision=2 WHERE id=$1", &[&repository.id]).unwrap();
    let cancelled = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &input,
        )
        .unwrap();
    let lease = store
        .claim_repository_job("cancelled-worker", 60)
        .unwrap()
        .unwrap();
    store
        .cancel_repository_job(
            &ctx,
            &project.id,
            &cancelled.id,
            &RepositoryJobChange {
                expected_revision: lease.job.revision,
            },
        )
        .unwrap();
    assert!(matches!(
        store.fail_repository_job(&lease, RepositoryJobFailure::SourceAccessDenied),
        Err(PlatformError::Conflict(_))
    ));
    assert_eq!(
        store
            .repository_job(&ctx, &project.id, &cancelled.id)
            .unwrap()
            .state,
        RepositoryJobState::Cancelled
    );
}
