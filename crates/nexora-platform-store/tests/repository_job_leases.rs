use nexora_platform::{
    Capability, GitProvider, KeyScope, PlatformError, RepositoryJobChange, RepositoryJobKind,
    RepositoryJobRequest, RepositoryJobState, RepositoryRegistration,
};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use std::{
    path::Path,
    sync::{Arc, Barrier},
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local PostgreSQL; competing workers, restart recovery and stale lease fencing"]
fn only_one_worker_claims_and_recovery_fences_expired_and_cancelled_attempts() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_job_lease_{}",
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
    let job = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &RepositoryJobRequest {
                expected_revision: 2,
            },
        )
        .unwrap();
    assert!(matches!(
        store.claim_repository_job("worker", 0),
        Err(PlatformError::Validation(_))
    ));
    let barrier = Arc::new(Barrier::new(3));
    let workers = (0..2)
        .map(|number| {
            let config = config.clone();
            let namespace = namespace.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let connected = PlatformStore::connect(&config, &namespace);
                barrier.wait();
                let mut store = connected.unwrap();
                store
                    .claim_repository_job(&format!("worker-{number}"), 60)
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let mut claims = workers
        .into_iter()
        .filter_map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        claims.len(),
        1,
        "Competing workers both claimed the same queued job"
    );
    let first = claims.pop().unwrap();
    assert_eq!(first.job.id, job.id);
    assert_eq!(first.job.state, RepositoryJobState::Running);
    assert_eq!(first.fence, 1);
    let renewed = store.renew_repository_job(&first, 60).unwrap();
    assert_eq!(renewed.fence, first.fence);
    assert!(
        store
            .claim_repository_job("another-worker", 60)
            .unwrap()
            .is_none()
    );
    drop(store);
    // Only this generated fixture's lease clock is advanced; no sleeps or production timeout changes.
    client.execute("UPDATE repository_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1", &[&job.id]).unwrap();
    let mut restarted = PlatformStore::connect(&config, &namespace).unwrap();
    let second = restarted
        .claim_repository_job("recovered-worker", 60)
        .unwrap()
        .unwrap();
    assert_eq!(second.job.id, first.job.id);
    assert_eq!(second.fence, first.fence + 1);
    assert_eq!(second.job.attempt, 2);
    assert!(matches!(
        restarted.renew_repository_job(&first, 60),
        Err(PlatformError::Conflict(_))
    ));
    let cancelled = restarted
        .cancel_repository_job(
            &ctx,
            &project.id,
            &job.id,
            &RepositoryJobChange {
                expected_revision: second.job.revision,
            },
        )
        .unwrap();
    assert_eq!(cancelled.state, RepositoryJobState::Cancelled);
    assert!(matches!(
        restarted.renew_repository_job(&second, 60),
        Err(PlatformError::Conflict(_))
    ));
    assert!(
        restarted
            .claim_repository_job("later-worker", 60)
            .unwrap()
            .is_none()
    );

    let pending = restarted
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &RepositoryJobRequest {
                expected_revision: 2,
            },
        )
        .unwrap();
    client.execute("UPDATE project_repositories SET authorization_status='revoked',revision=revision+1 WHERE id=$1", &[&repository.id]).unwrap();
    assert!(
        restarted
            .claim_repository_job("source-revoked", 60)
            .unwrap()
            .is_none()
    );
    let rejected = restarted
        .repository_job(&ctx, &project.id, &pending.id)
        .unwrap();
    assert_eq!(rejected.state, RepositoryJobState::Failed);
    assert_eq!(rejected.error_code.as_deref(), Some("source_changed"));
    assert_eq!(rejected.attempt, 0);
    client.execute("UPDATE project_repositories SET authorization_status='authorized',revision=revision+1 WHERE id=$1", &[&repository.id]).unwrap();
    let revision = restarted
        .get_repository(&ctx, &project.id, &repository.id)
        .unwrap()
        .revision;
    let input = RepositoryJobRequest {
        expected_revision: revision,
    };
    let key = restarted
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Queued job origin",
            KeyScope {
                capabilities: vec![Capability::RepositoriesSync, Capability::RepositoriesRead],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let key_ctx = restarted.authenticate(&key.key).unwrap();
    let pending = restarted
        .enqueue_repository_job(
            &key_ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &input,
        )
        .unwrap();
    restarted.revoke_key(&ctx, &key.metadata.id).unwrap();
    assert!(
        restarted
            .claim_repository_job("credential-revoked", 60)
            .unwrap()
            .is_none()
    );
    let rejected = restarted
        .repository_job(&ctx, &project.id, &pending.id)
        .unwrap();
    assert_eq!(rejected.state, RepositoryJobState::Failed);
    assert_eq!(rejected.error_code.as_deref(), Some("authority_revoked"));
    assert_eq!(rejected.attempt, 0);

    let bounded = restarted
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &input,
        )
        .unwrap();
    for attempt in 1..=3 {
        let lease = restarted
            .claim_repository_job("recovering-worker", 60)
            .unwrap()
            .unwrap();
        assert_eq!(lease.job.id, bounded.id);
        assert_eq!(lease.fence, attempt);
        client.execute("UPDATE repository_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1", &[&bounded.id]).unwrap();
    }
    assert!(
        restarted
            .claim_repository_job("exhausted-worker", 60)
            .unwrap()
            .is_none()
    );
    let exhausted = restarted
        .repository_job(&ctx, &project.id, &bounded.id)
        .unwrap();
    assert_eq!(exhausted.state, RepositoryJobState::Failed);
    assert_eq!(exhausted.attempt, 3);
    assert_eq!(exhausted.error_code.as_deref(), Some("attempts_exhausted"));

    let changing = restarted
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &input,
        )
        .unwrap();
    let lease = restarted
        .claim_repository_job("source-generation-worker", 60)
        .unwrap()
        .unwrap();
    assert_eq!(lease.job.id, changing.id);
    client
        .execute(
            "UPDATE project_repositories SET source_generation=source_generation+1 WHERE id=$1",
            &[&repository.id],
        )
        .unwrap();
    assert!(matches!(
        restarted.renew_repository_job(&lease, 60),
        Err(PlatformError::Conflict(_))
    ));
    let rejected = restarted
        .repository_job(&ctx, &project.id, &changing.id)
        .unwrap();
    assert_eq!(rejected.state, RepositoryJobState::Failed);
    assert_eq!(rejected.error_code.as_deref(), Some("source_changed"));
}
