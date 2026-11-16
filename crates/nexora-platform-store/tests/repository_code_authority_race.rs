#[path = "support/neo4j_read_gate.rs"]
mod neo4j_read_gate;
use neo4j_read_gate::Neo4jReadGate;
use nexora_core::{RepositoryId, TenantId};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use nexora_platform::{
    Capability, GitProvider, KeyScope, PlatformError, RepositoryJobKind, RepositoryJobRequest,
    RepositoryRegistration,
};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::{DatabaseConnections, DatabaseRuntime, KnowledgeQuery};
use std::{
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local databases; holds a real graph read and observes key revocation waiting on its SQL authority lock"]
fn key_revocation_linearizes_after_in_flight_code_read_and_denies_warm_followup() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let namespace = format!("nexora_platform_code_race_{nonce}");
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Publication fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let verified = store
        .identity_gate(&session.access_token, "skip", "")
        .unwrap();
    let ctx = store
        .authenticate(verified["access_token"].as_str().unwrap())
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

    store.complete_repository_job(&first, &header).unwrap();
    let key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Read race fixture",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead, Capability::KnowledgeRead],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    drop(runtime);
    let mut gate = Neo4jReadGate::start(config.neo4j_http_port);
    let mut read_config = config.clone();
    read_config.neo4j_http_port = gate.port;
    let mut reader = PlatformStore::connect(&read_config, &namespace).unwrap();
    let reader_ctx = reader.authenticate(&key.key).unwrap();
    let query = KnowledgeQuery::FindSymbol {
        name: "published".into(),
        offset: 0,
        limit: 10,
    };
    let mut revoker = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(pg_password.trim())
        .dbname(&config.postgres_database)
        .options(&format!(
            "-c search_path={namespace},pg_catalog -c statement_timeout=15000"
        ))
        .connect(postgres::NoTls)
        .unwrap();
    let revoke_pid: i32 = revoker
        .query_one("SELECT pg_backend_pid()", &[])
        .unwrap()
        .get(0);
    let (mut reader, value, observed, blocked, revoked) = std::thread::scope(|threads| {
        let read_ctx = &reader_ctx;
        let read_query = &query;
        let read_project = &project.id;
        let read_repository = &repository.id;
        let read = threads.spawn(move || {
            let result = reader.query_repository_code(
                read_ctx,
                read_project,
                read_repository,
                None,
                read_query,
            );
            (reader, result)
        });
        let observed = gate.wait_read();
        let key_id = &key.metadata.id;
        let revoke = threads.spawn(move || revoker.execute(
            "UPDATE api_keys SET revoked_at=clock_timestamp() WHERE id=$1 AND revoked_at IS NULL", &[&key_id],
        ));
        let deadline = Instant::now() + Duration::from_secs(10);
        let blocked = loop {
            let waiting: bool = sql
                .query_one("SELECT cardinality(pg_blocking_pids($1))>0", &[&revoke_pid])
                .unwrap()
                .get(0);
            if waiting {
                break true;
            }
            if revoke.is_finished() || Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        gate.release();
        let (reader, result) = read.join().unwrap();
        (reader, result, observed, blocked, revoke.join().unwrap())
    });
    assert!(observed, "Fixture did not observe the backend read");
    assert!(
        blocked,
        "Revocation did not wait for the in-flight authority lock"
    );
    let value = value.unwrap();
    assert!(!value["items"].as_array().unwrap().is_empty());
    assert_eq!(revoked.unwrap(), 1);
    assert!(
        matches!(
            reader.query_repository_code(&reader_ctx, &project.id, &repository.id, None, &query),
            Err(PlatformError::Unauthorized)
        ),
        "A warm result must not authorize a revoked key"
    );
    std::fs::remove_dir_all(root).unwrap();
}
