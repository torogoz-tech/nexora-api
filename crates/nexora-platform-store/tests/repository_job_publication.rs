use nexora_core::{RepositoryId, TenantId};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use nexora_platform::{
    Capability, GitProvider, PlatformError, RepositoryJobChange, RepositoryJobKind,
    RepositoryJobRequest, RepositoryJobState, RepositoryRegistration,
};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::{DatabaseConnections, DatabaseRuntime};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local databases; atomic publication, immutable commit, fencing and live SQL authority"]
fn publication_is_atomic_scoped_replay_safe_and_rejects_cancelled_expired_or_revoked_work() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let namespace = format!("nexora_platform_publish_{nonce}");
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Publication fixture", "owner", &password, "Owner")
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
    let first = store
        .claim_repository_job("publisher", 300)
        .unwrap()
        .unwrap();
    assert_eq!(first.job.id, job.id);
    assert!(matches!(
        store.pin_repository_job(&first, "invalid"),
        Err(PlatformError::Validation(_))
    ));
    let commit = "a".repeat(40);
    let scope = store.pin_repository_job(&first, &commit).unwrap();
    let installation: String = sql
        .query_one("SELECT id FROM installation WHERE singleton", &[])
        .unwrap()
        .get(0);
    assert_eq!(scope.tenant_id, installation);
    assert_eq!(scope.repository_id, repository.id);
    assert_eq!(scope.pinned_commit, commit);
    assert!(matches!(
        store.pin_repository_job(&first, &"b".repeat(40)),
        Err(PlatformError::Conflict(_))
    ));
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-publication-{nonce}"));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("source.js"), "function published() {}\n").unwrap();
    let indexer =
        Indexer::new(LanguageDefinition::load(Path::new("../../languages/javascript")).unwrap());
    let mut runtime = DatabaseRuntime::connect(&config).unwrap();
    let tenant = TenantId(scope.tenant_id.clone());
    let runtime_repository = RepositoryId(scope.repository_id.clone());
    let staged = runtime
        .stage_repository(&indexer, &root, tenant.clone(), runtime_repository.clone())
        .unwrap();
    let header = runtime
        .query_header(&tenant, &runtime_repository, Some(&staged.snapshot.id))
        .unwrap();
    assert!(
        runtime
            .snapshot(&tenant, &runtime_repository, None)
            .unwrap()
            .is_none()
    );
    let mut wrong = header.clone();
    wrong.tenant_id = TenantId("unrelated".into());
    assert!(matches!(
        store.complete_repository_job(&first, &wrong),
        Err(PlatformError::Conflict(_))
    ));
    wrong = header.clone();
    wrong.payload_hash = "0".repeat(64);
    assert!(matches!(
        store.complete_repository_job(&first, &wrong),
        Err(PlatformError::Conflict(_))
    ));
    sql.execute("UPDATE nexora_runtime.query_snapshots SET version=0 WHERE tenant_id=$1 AND repository_id=$2 AND snapshot_id=$3", &[&tenant.0,&runtime_repository.0,&header.id.0]).unwrap();
    assert!(matches!(
        store.complete_repository_job(&first, &header),
        Err(PlatformError::Conflict(_))
    ));
    sql.execute("UPDATE nexora_runtime.query_snapshots SET version=$4 WHERE tenant_id=$1 AND repository_id=$2 AND snapshot_id=$3", &[&tenant.0,&runtime_repository.0,&header.id.0,&nexora_runtime::QuerySnapshot::VERSION]).unwrap();
    assert_eq!(
        sql.query_one("SELECT count(*) FROM repository_bindings", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );

    // A failure in this generated schema's audit insert must roll back both publication and job completion.
    sql.batch_execute("CREATE FUNCTION publication_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='repository.job_completed' THEN RAISE EXCEPTION 'generated publication failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER publication_failure BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION publication_failure()").unwrap();
    assert!(matches!(
        store.complete_repository_job(&first, &header),
        Err(PlatformError::Storage)
    ));
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
    sql.batch_execute(
        "DROP TRIGGER publication_failure ON audit_events; DROP FUNCTION publication_failure()",
    )
    .unwrap();
    sql.batch_execute("CREATE FUNCTION publication_expiry() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='repository.job_completed' THEN UPDATE repository_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE id=NEW.resource_id; END IF; RETURN NEW; END $$; CREATE TRIGGER publication_expiry BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION publication_expiry()").unwrap();
    assert!(matches!(
        store.complete_repository_job(&first, &header),
        Err(PlatformError::Conflict(_))
    ));
    assert_eq!(
        sql.query_one("SELECT count(*) FROM repository_bindings", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    sql.batch_execute(
        "DROP TRIGGER publication_expiry ON audit_events; DROP FUNCTION publication_expiry()",
    )
    .unwrap();
    let binding = store.complete_repository_job(&first, &header).unwrap();
    assert_eq!(binding.snapshot_id, header.id.0);
    assert_eq!(binding.pinned_commit, commit);
    assert_eq!(binding.job_id, job.id);
    let replay = store.complete_repository_job(&first, &header).unwrap();
    assert_eq!(replay.revision, binding.revision);
    let encoded = serde_json::to_string(&binding).unwrap();
    assert!(!encoded.contains("completion_digest"));
    assert!(!encoded.contains("lease_token"));
    assert_eq!(
        sql.query_one(
            "SELECT count(*) FROM audit_events WHERE operation='repository.job_completed'",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        1
    );
    assert_eq!(
        store
            .repository_job(&ctx, &project.id, &job.id)
            .unwrap()
            .state,
        RepositoryJobState::Succeeded
    );
    drop(store);
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    assert_eq!(
        store
            .complete_repository_job(&first, &header)
            .unwrap()
            .revision,
        binding.revision
    );

    let next = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Index,
            &input,
        )
        .unwrap();
    let expired = store.claim_repository_job("expired", 300).unwrap().unwrap();
    store.pin_repository_job(&expired, &commit).unwrap();
    sql.execute(
        "UPDATE repository_jobs SET lease_until=clock_timestamp()-interval '1 second' WHERE id=$1",
        &[&next.id],
    )
    .unwrap();
    assert!(matches!(
        store.complete_repository_job(&expired, &header),
        Err(PlatformError::Conflict(_))
    ));
    let recovered = store
        .claim_repository_job("recovered", 300)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.fence, expired.fence + 1);
    assert_eq!(
        store
            .pin_repository_job(&recovered, &commit)
            .unwrap()
            .pinned_commit,
        commit
    );
    assert!(matches!(
        store.complete_repository_job(&expired, &header),
        Err(PlatformError::Conflict(_))
    ));
    let newer = store.complete_repository_job(&recovered, &header).unwrap();
    assert_eq!(newer.revision, binding.revision + 1);
    assert!(matches!(
        store.complete_repository_job(&first, &header),
        Err(PlatformError::Conflict(_))
    ));
    assert_eq!(
        sql.query_one(
            "SELECT job_id FROM repository_bindings WHERE repository_id=$1",
            &[&repository.id]
        )
        .unwrap()
        .get::<_, String>(0),
        next.id
    );

    let cancelled = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &input,
        )
        .unwrap();
    let cancelled_lease = store
        .claim_repository_job("cancelled", 300)
        .unwrap()
        .unwrap();
    store.pin_repository_job(&cancelled_lease, &commit).unwrap();
    let cancelled_revision = store
        .repository_job(&ctx, &project.id, &cancelled.id)
        .unwrap()
        .revision;
    store
        .cancel_repository_job(
            &ctx,
            &project.id,
            &cancelled.id,
            &RepositoryJobChange {
                expected_revision: cancelled_revision,
            },
        )
        .unwrap();
    assert!(matches!(
        store.complete_repository_job(&cancelled_lease, &header),
        Err(PlatformError::Conflict(_))
    ));

    let changed = store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Sync,
            &input,
        )
        .unwrap();
    let changed_lease = store.claim_repository_job("changed", 300).unwrap().unwrap();
    store.pin_repository_job(&changed_lease, &commit).unwrap();
    sql.execute(
        "UPDATE project_repositories SET source_generation=source_generation+1 WHERE id=$1",
        &[&repository.id],
    )
    .unwrap();
    assert!(matches!(
        store.complete_repository_job(&changed_lease, &header),
        Err(PlatformError::Conflict(_))
    ));
    let changed_revision = store
        .repository_job(&ctx, &project.id, &changed.id)
        .unwrap()
        .revision;
    store
        .cancel_repository_job(
            &ctx,
            &project.id,
            &changed.id,
            &RepositoryJobChange {
                expected_revision: changed_revision,
            },
        )
        .unwrap();
    store
        .enqueue_repository_job(
            &ctx,
            &project.id,
            &repository.id,
            RepositoryJobKind::Index,
            &input,
        )
        .unwrap();
    let revoked = store.claim_repository_job("revoked", 300).unwrap().unwrap();
    store.pin_repository_job(&revoked, &commit).unwrap();
    assert!(
        matches!(
            store.revoke_grant(&ctx, ctx.actor_id(), Capability::RepositoriesSync, None),
            Err(PlatformError::Forbidden)
        ),
        "human permissions are managed through membership roles"
    );
    // Inject a live authority loss in this isolated fixture, as in publication race tests.
    assert_eq!(
        sql.execute(
            "DELETE FROM actor_grants WHERE actor_id=$1 AND capability='repositories:sync' AND project_id IS NULL",
            &[&ctx.actor_id()],
        ).unwrap(),
        1
    );
    assert!(matches!(
        store.complete_repository_job(&revoked, &header),
        Err(PlatformError::Forbidden)
    ));
    assert_eq!(
        sql.query_one(
            "SELECT revision FROM repository_bindings WHERE repository_id=$1",
            &[&repository.id]
        )
        .unwrap()
        .get::<_, i64>(0),
        newer.revision
    );
    std::fs::remove_dir_all(root).unwrap();
}
