use nexora_platform::{
    GitProvider, PlatformError, RepositoryJobChange, RepositoryJobKind, RepositoryJobRequest,
    RepositoryJobState, RepositoryRegistration,
};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::json;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local PostgreSQL; durable repository job lifecycle, no source execution"]
fn jobs_survive_restart_and_replay_without_repeating_or_crossing_project_boundaries() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_jobs_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Job fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let project = store.create_project(&ctx, "Source", None).unwrap();
    let other = store.create_project(&ctx, "Other", None).unwrap();
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
    let input = RepositoryJobRequest {
        expected_revision: 1,
    };
    assert!(
        matches!(
            store.enqueue_repository_job(
                &ctx,
                &project.id,
                &repository.id,
                RepositoryJobKind::Sync,
                &input
            ),
            Err(PlatformError::Conflict(_))
        ),
        "A registration without source authorization cannot enqueue work"
    );

    // Only this generated schema receives representative authority rows. No provider token is used.
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
    let payload = json!({"project_id":project.id,"repository_id":repository.id,"request":input});
    let job = store
        .request(&ctx, "repository.sync", "enqueue-once", &payload, |s| {
            s.enqueue_repository_job(
                &ctx,
                &project.id,
                &repository.id,
                RepositoryJobKind::Sync,
                &input,
            )
        })
        .unwrap();
    assert_eq!(job.state, RepositoryJobState::Queued);
    assert_eq!(job.attempt, 0);
    assert_eq!(job.revision, 1);
    assert_eq!(job.repository_id, repository.id);
    assert_eq!(job.requested_by, ctx.actor_id());
    assert!(matches!(
        store.repository_job(&ctx, &other.id, &job.id),
        Err(PlatformError::NotFound)
    ));
    assert!(matches!(
        store.enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &RepositoryJobRequest {
                expected_revision: 1
            }
        ),
        Err(PlatformError::Conflict(_))
    ));
    drop(store);

    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let loaded = store.repository_job(&ctx, &project.id, &job.id).unwrap();
    assert_eq!(loaded.id, job.id);
    assert_eq!(loaded.state, RepositoryJobState::Queued);
    let replay = store
        .request(&ctx, "repository.sync", "enqueue-once", &payload, |s| {
            s.enqueue_repository_job(
                &ctx,
                &project.id,
                &repository.id,
                RepositoryJobKind::Sync,
                &input,
            )
        })
        .unwrap();
    assert_eq!(replay.id, job.id);
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM repository_jobs", &[])
            .unwrap()
            .get::<_, i64>(0),
        1
    );

    let cancelled = store
        .cancel_repository_job(
            &ctx,
            &project.id,
            &job.id,
            &RepositoryJobChange {
                expected_revision: 1,
            },
        )
        .unwrap();
    assert_eq!(cancelled.state, RepositoryJobState::Cancelled);
    assert_eq!(cancelled.revision, 2);
    assert!(matches!(
        store.cancel_repository_job(
            &ctx,
            &project.id,
            &job.id,
            &RepositoryJobChange {
                expected_revision: 1
            }
        ),
        Err(PlatformError::Conflict(_))
    ));
    let historical = store
        .request(&ctx, "repository.sync", "enqueue-once", &payload, |s| {
            s.enqueue_repository_job(
                &ctx,
                &project.id,
                &repository.id,
                RepositoryJobKind::Sync,
                &input,
            )
        })
        .unwrap();
    assert_eq!(historical.state, RepositoryJobState::Queued);
    assert_eq!(
        store
            .repository_job(&ctx, &project.id, &job.id)
            .unwrap()
            .state,
        RepositoryJobState::Cancelled
    );
    drop(store);
    let mut restarted = PlatformStore::connect(&config, &namespace).unwrap();
    restarted.initialize().unwrap();
    assert_eq!(
        restarted
            .repository_job(&ctx, &project.id, &job.id)
            .unwrap()
            .state,
        RepositoryJobState::Cancelled
    );
}
