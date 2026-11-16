use nexora_platform::{
    ActorKind, Capability, GitProvider, KeyScope, PlatformError, RepositoryJobKind,
    RepositoryJobRequest, RepositoryRegistration,
};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use std::{
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local PostgreSQL; real blocking revocation and stale enqueue authority"]
fn enqueue_rechecks_grants_after_a_row_lock_wait_and_rejects_revoked_credentials() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_job_auth_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Job authority fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let owner = store.authenticate(&session.access_token).unwrap();
    let project = store.create_project(&owner, "Source", None).unwrap();
    let repository = store
        .register_repository(
            &owner,
            &project.id,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "fixture/source".into(),
            },
        )
        .unwrap();
    let actor = store
        .create_actor(
            &owner,
            ActorKind::Agent,
            "Worker requester",
            "Job authority fixture",
        )
        .unwrap();
    store
        .grant(
            &owner,
            &actor.id,
            Capability::RepositoriesSync,
            Some(&project.id),
        )
        .unwrap();
    let key = store
        .issue_key(
            &owner,
            &actor.id,
            "Job authority fixture",
            KeyScope {
                capabilities: vec![Capability::RepositoriesSync],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let stale = store.authenticate(&key.key).unwrap();
    let input = RepositoryJobRequest {
        expected_revision: 1,
    };
    assert!(
        matches!(
            store.enqueue_repository_job(
                &stale,
                &project.id,
                &repository.id,
                RepositoryJobKind::Sync,
                &input
            ),
            Err(PlatformError::Conflict(_))
        ),
        "Authorized request should reach source-binding validation"
    );

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
    let backend: i32 = client
        .query_one("SELECT pg_backend_pid()", &[])
        .unwrap()
        .get(0);
    let mut tx = client.transaction().unwrap();
    assert_eq!(tx.execute("DELETE FROM actor_grants WHERE actor_id=$1 AND capability='repositories:sync' AND project_id=$2", &[&actor.id,&project.id]).unwrap(), 1);
    let worker_config = config.clone();
    let worker_namespace = namespace.clone();
    let worker_ctx = stale.clone();
    let worker_project = project.id.clone();
    let worker_repository = repository.id.clone();
    let worker = std::thread::spawn(move || {
        let mut store = PlatformStore::connect(&worker_config, &worker_namespace).unwrap();
        store.enqueue_repository_job(
            &worker_ctx,
            &worker_project,
            &worker_repository,
            RepositoryJobKind::Sync,
            &RepositoryJobRequest {
                expected_revision: 1,
            },
        )
    });
    // Observe a genuine SQL lock wait, rather than guessing from a sleep duration.
    let deadline = Instant::now() + Duration::from_secs(20);
    let blocked = loop {
        // PostgreSQL retains activity statistics for the observing transaction.
        tx.query_one("SELECT pg_stat_clear_snapshot()", &[])
            .unwrap();
        let waiting: bool = tx
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
                &[&backend],
            )
            .unwrap()
            .get(0);
        if waiting {
            break true;
        }
        if worker.is_finished() || Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    tx.commit().unwrap();
    let result = worker.join().unwrap();
    assert!(
        blocked,
        "Fixture did not observe the enqueue waiting on revocation"
    );
    assert!(
        matches!(result, Err(PlatformError::Forbidden)),
        "Enqueue used authority from before the revocation committed"
    );
    assert_eq!(
        client
            .query_one("SELECT count(*) FROM repository_jobs", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );

    store
        .grant(
            &owner,
            &actor.id,
            Capability::RepositoriesSync,
            Some(&project.id),
        )
        .unwrap();
    assert!(matches!(
        store.enqueue_repository_job(
            &stale,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &input
        ),
        Err(PlatformError::Conflict(_))
    ));
    store.revoke_key(&owner, &key.metadata.id).unwrap();
    assert!(matches!(
        store.enqueue_repository_job(
            &stale,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &input
        ),
        Err(PlatformError::Unauthorized)
    ));
}
