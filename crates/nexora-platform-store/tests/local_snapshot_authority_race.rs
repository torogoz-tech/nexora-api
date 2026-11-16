#[path = "support/local_snapshot_fixture.rs"]
mod local_snapshot_fixture;
#[path = "support/neo4j_read_gate.rs"]
mod neo4j_read_gate;
#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use local_snapshot_fixture::LocalSnapshotFixture;
use neo4j_read_gate::Neo4jReadGate;
use nexora_platform::{Capability, KeyScope, PlatformError};
use nexora_platform_store::PlatformStore;
use nexora_runtime::KnowledgeQuery;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local databases; real held graph read blocks local trust revocation and warm followup is denied"]
fn local_source_revocation_linearizes_after_the_authorized_backend_read() {
    let mut f = LocalSnapshotFixture::new();
    let publication = f.adopt();
    let project = f.work.project.id.clone();
    let repository = publication.repository_id;
    assert_eq!(
        f.runtime
            .query_header(&f.header.tenant_id, &f.header.repository_id, None)
            .unwrap()
            .id,
        f.header.id
    );
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Local race reader",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead, Capability::KnowledgeRead],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let mut gate = Neo4jReadGate::start(f.work.config.neo4j_http_port);
    let mut read_config = f.work.config.clone();
    read_config.neo4j_http_port = gate.port;
    let mut reader = PlatformStore::connect(&read_config, &f.work.namespace).unwrap();
    let reader_ctx = reader.authenticate(&key.key).unwrap();
    let query = KnowledgeQuery::FindSymbol {
        name: "retained_identity".into(),
        offset: 0,
        limit: 10,
    };
    let password = Zeroizing::new(
        std::fs::read_to_string(f.work.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut revoker = postgres::Config::new()
        .host(&f.work.config.host)
        .port(f.work.config.postgres_port)
        .user(&f.work.config.postgres_user)
        .password(password.trim())
        .dbname(&f.work.config.postgres_database)
        .options(&format!(
            "-c search_path={},pg_catalog -c statement_timeout=15000",
            f.work.namespace
        ))
        .connect(postgres::NoTls)
        .unwrap();
    let pid: i32 = revoker
        .query_one("SELECT pg_backend_pid()", &[])
        .unwrap()
        .get(0);
    let (mut reader, read, observed, blocked, revoked) = std::thread::scope(|threads| {
        let project = &project;
        let repository = &repository;
        let reader_ctx = &reader_ctx;
        let query = &query;
        let read = threads.spawn(move || {
            let result = reader.query_repository_code(reader_ctx, project, repository, None, query);
            (reader, result)
        });
        let observed = gate.wait_read();
        // Fixture-only physical revocation exercises the exact resource lock without acquiring
        // unrelated operator-session locks first. Native lifecycle transitions have separate acceptance.
        let revoke = threads.spawn(move || -> Result<(),postgres::Error> {
            let mut tx = revoker.transaction()?;
            tx.execute("UPDATE local_repository_sources SET state='revoked',authorization_revision=authorization_revision+1,source_generation=source_generation+1,revision=revision+1 WHERE repository_id=$1", &[&repository])?;
            tx.execute("UPDATE project_repositories SET authorization_status='revoked',source_generation=source_generation+1,revision=revision+1 WHERE id=$1", &[&repository])?;
            tx.commit()
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        let blocked = loop {
            let blocked: bool = f
                .sql
                .query_one("SELECT cardinality(pg_blocking_pids($1))>0", &[&pid])
                .unwrap()
                .get(0);
            if blocked {
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
    assert!(observed, "fixture did not observe the real graph request");
    assert!(
        blocked,
        "source revocation bypassed the in-flight source lock"
    );
    assert!(!read.unwrap()["items"].as_array().unwrap().is_empty());
    revoked.unwrap();
    assert!(
        matches!(
            reader.query_repository_code(&reader_ctx, &project, &repository, None, &query),
            Err(PlatformError::NotFound)
        ),
        "a warm result must not revive retired local trust"
    );
    std::fs::remove_dir_all(&f.root).unwrap();
}
