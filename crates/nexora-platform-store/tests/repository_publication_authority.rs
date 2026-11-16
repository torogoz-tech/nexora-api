use nexora_core::{RepositoryId, TenantId};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use nexora_platform::{
    Capability, GitProvider, PlatformError, RepositoryJobKind, RepositoryJobRequest,
    RepositoryJobState, RepositoryRegistration,
};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::{DatabaseConnections, DatabaseRuntime};
use std::{
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local databases; observe both SQL orderings of publication versus grant revocation"]
fn publication_and_grant_revocation_have_one_observed_transactional_order() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let namespace = format!("nexora_platform_publish_race_{nonce}");
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Publication authority fixture", "owner", &password, "Owner")
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
    let connection = SecretGenerator::id("conn").unwrap();
    sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab','gitlab.com',$2,'42','fixture','active',1,'{}',now()+interval '1 hour')", &[&connection,&ctx.actor_id()]).unwrap();
    sql.execute("UPDATE project_repositories SET remote_id='101',remote_full_name='fixture/source',connection_id=$1,authorization_status='authorized',revision=2 WHERE id=$2", &[&connection,&repository.id]).unwrap();
    let job = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Index,
            &RepositoryJobRequest {
                expected_revision: 2,
            },
        )
        .unwrap();
    let lease = store
        .claim_repository_job("publication-race", 300)
        .unwrap()
        .unwrap();
    let scope = store.pin_repository_job(&lease, &"a".repeat(40)).unwrap();
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-publication-race-{nonce}"));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("source.js"), "function fixture() {}\n").unwrap();
    let indexer =
        Indexer::new(LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap());
    let tenant = TenantId(scope.tenant_id);
    let runtime_repository = RepositoryId(scope.repository_id);
    let mut runtime = DatabaseRuntime::connect(&config).unwrap();
    let staged = runtime
        .stage_repository(&indexer, &root, tenant.clone(), runtime_repository.clone())
        .unwrap();
    let header = runtime
        .query_header(&tenant, &runtime_repository, Some(&staged.snapshot.id))
        .unwrap();
    let backend: i32 = sql
        .query_one("SELECT pg_backend_pid()", &[])
        .unwrap()
        .get(0);

    // Revocation owns the grant first. Publication must wait, then observe the committed deletion.
    let first_lease = store.renew_repository_job(&lease, 300).unwrap();
    let mut revocation = sql.transaction().unwrap();
    assert_eq!(revocation.execute("DELETE FROM actor_grants WHERE actor_id=$1 AND capability='repositories:sync' AND project_id IS NULL", &[&ctx.actor_id()]).unwrap(), 1);
    let worker_config = config.clone();
    let worker_namespace = namespace.clone();
    let first_header = header.clone();
    let first = std::thread::spawn(move || {
        let mut publisher = PlatformStore::connect(&worker_config, &worker_namespace).unwrap();
        publisher.complete_repository_job(&first_lease, &first_header)
    });
    let deadline = Instant::now() + Duration::from_secs(8);
    let publication_waited = loop {
        revocation
            .query_one("SELECT pg_stat_clear_snapshot()", &[])
            .unwrap();
        let waiting: bool = revocation
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
                &[&backend],
            )
            .unwrap()
            .get(0);
        if waiting {
            break true;
        }
        if first.is_finished() || Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    revocation.commit().unwrap();
    let denied = first.join().unwrap();
    assert!(
        publication_waited,
        "Did not observe publication waiting for grant revocation"
    );
    assert!(matches!(denied, Err(PlatformError::Forbidden)));
    assert_eq!(
        sql.query_one("SELECT count(*) FROM repository_bindings", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    assert_eq!(
        store
            .repository_job(&ctx, &project.id, &job.id)
            .unwrap()
            .state,
        RepositoryJobState::Running
    );

    // Publication owns the authority first. A controlled fixture barrier holds it before final commit.
    assert!(matches!(
        store.grant(&ctx, ctx.actor_id(), Capability::RepositoriesSync, None),
        Err(PlatformError::Forbidden)
    ));
    // Restore only this generated fixture's baseline for the opposite ordering.
    assert_eq!(sql.execute("INSERT INTO actor_grants(actor_id,capability,project_id) VALUES($1,'repositories:sync',NULL)", &[&ctx.actor_id()]).unwrap(), 1);
    let second_lease = store.renew_repository_job(&lease, 300).unwrap();
    let barrier = (nonce % i64::MAX as u128) as i64;
    sql.batch_execute(&format!("CREATE FUNCTION publication_barrier() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='repository.job_completed' THEN PERFORM pg_advisory_xact_lock({barrier}); END IF; RETURN NEW; END $$; CREATE TRIGGER publication_barrier BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION publication_barrier()")).unwrap();
    sql.query_one("SELECT pg_advisory_lock($1)", &[&barrier])
        .unwrap();
    let worker_config = config.clone();
    let worker_namespace = namespace.clone();
    let second_header = header.clone();
    let second = std::thread::spawn(move || {
        let mut publisher = PlatformStore::connect(&worker_config, &worker_namespace).unwrap();
        publisher.complete_repository_job(&second_lease, &second_header)
    });
    let deadline = Instant::now() + Duration::from_secs(8);
    let publisher_pid = loop {
        sql.query_one("SELECT pg_stat_clear_snapshot()", &[])
            .unwrap();
        let waiting = sql.query_opt("SELECT pid FROM pg_locks WHERE locktype='advisory' AND NOT granted AND $1=ANY(pg_blocking_pids(pid))", &[&backend]).unwrap();
        if let Some(row) = waiting {
            break Some(row.get::<_, i32>(0));
        }
        if second.is_finished() || Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut remover = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(pg_password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    remover
        .batch_execute(&format!(
            "SET search_path TO \"{namespace}\"; SET statement_timeout='8s'"
        ))
        .unwrap();
    let remover_pid: i32 = remover
        .query_one("SELECT pg_backend_pid()", &[])
        .unwrap()
        .get(0);
    let actor = ctx.actor_id().to_owned();
    let removal = std::thread::spawn(move || {
        remover.execute("DELETE FROM actor_grants WHERE actor_id=$1 AND capability='repositories:sync' AND project_id IS NULL", &[&actor])
    });
    let deadline = Instant::now() + Duration::from_secs(4);
    let revocation_waited = loop {
        sql.query_one("SELECT pg_stat_clear_snapshot()", &[])
            .unwrap();
        let waiting: bool = sql
            .query_one(
                "SELECT $1=ANY(pg_blocking_pids($2))",
                &[&publisher_pid.unwrap_or(-1), &remover_pid],
            )
            .unwrap()
            .get(0);
        if waiting {
            break true;
        }
        if removal.is_finished() || Instant::now() >= deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    sql.query_one("SELECT pg_advisory_unlock($1)", &[&barrier])
        .unwrap();
    let published = second.join().unwrap();
    let removed = removal.join().unwrap().unwrap();
    assert!(
        publisher_pid.is_some(),
        "Did not observe publication at its controlled SQL barrier"
    );
    assert!(
        revocation_waited,
        "Grant deletion did not wait for the publication's authority locks"
    );
    assert_eq!(removed, 1);
    let binding = published.unwrap();
    assert_eq!(binding.job_id, job.id);
    assert_eq!(
        store
            .repository_job(&ctx, &project.id, &job.id)
            .unwrap()
            .state,
        RepositoryJobState::Succeeded
    );
    assert!(matches!(
        store.complete_repository_job(&lease, &header),
        Err(PlatformError::Forbidden)
    ));
    assert_eq!(
        sql.query_one(
            "SELECT count(*) FROM audit_events WHERE operation='repository.job_completed'",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        1
    );
    sql.batch_execute(
        "DROP TRIGGER publication_barrier ON audit_events; DROP FUNCTION publication_barrier()",
    )
    .unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
