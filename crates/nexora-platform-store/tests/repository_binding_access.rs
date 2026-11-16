use nexora_core::{RepositoryId, TenantId};
use nexora_indexer::Indexer;
use nexora_language::LanguageDefinition;
use nexora_platform::{
    ActorKind, Capability, GitProvider, KeyScope, PlatformError, RepositoryJobKind,
    RepositoryJobRequest, RepositoryRegistration,
};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::{DatabaseConnections, DatabaseRuntime, KnowledgeQuery};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local databases; registered code reads must resolve current binding with live actor, key and source authority"]
fn binding_reads_use_logical_ids_and_recheck_key_source_and_project_authority() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let namespace = format!("nexora_platform_binding_read_{nonce}");
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

    assert!(
        store
            .repository_binding(&ctx, &project.id, &repository.id)
            .is_err()
    );
    let published = store.complete_repository_job(&first, &header).unwrap();
    let member = store
        .create_actor(
            &ctx,
            ActorKind::Agent,
            "Project reader",
            "Read-only fixture",
        )
        .unwrap();
    for capability in [
        Capability::RepositoriesRead,
        Capability::KnowledgeRead,
        Capability::RepositoriesSync,
    ] {
        store
            .grant(&ctx, &member.id, capability, Some(&project.id))
            .unwrap();
    }
    let key = store
        .issue_key(
            &ctx,
            &member.id,
            "Read registered code",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead, Capability::KnowledgeRead],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let reader = store.authenticate(&key.key).unwrap();
    let binding = store
        .repository_binding(&reader, &project.id, &repository.id)
        .unwrap();
    assert_eq!(binding.snapshot_id, published.snapshot_id);
    assert_eq!(binding.runtime_tenant_id, installation);
    assert_eq!(binding.runtime_repository_id, repository.id);
    let query = KnowledgeQuery::FindSymbol {
        name: "published".into(),
        offset: 0,
        limit: 10,
    };
    assert!(
        store
            .query_repository_code(&reader, &project.id, &repository.id, None, &query)
            .is_ok()
    );
    let sync_key = store
        .issue_key(
            &ctx,
            &member.id,
            "Sync without delegation",
            KeyScope {
                capabilities: vec![Capability::RepositoriesSync],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let sync_reader = store.authenticate(&sync_key.key).unwrap();
    assert!(
        matches!(
            store.enqueue_repository_job(
                &sync_reader,
                &project.id,
                &repository.id,
                RepositoryJobKind::Sync,
                &input
            ),
            Err(PlatformError::Forbidden)
        ),
        "Reading published code must not delegate provider credentials"
    );
    sql.execute(
        "DELETE FROM actor_grants WHERE actor_id=$1 AND capability='knowledge:read'",
        &[&member.id],
    )
    .unwrap();
    assert!(matches!(
        store.repository_binding(&reader, &project.id, &repository.id),
        Err(PlatformError::NotFound)
    ));
    store
        .grant(
            &ctx,
            &member.id,
            Capability::KnowledgeRead,
            Some(&project.id),
        )
        .unwrap();
    assert!(
        store
            .repository_binding(&reader, &project.id, &repository.id)
            .is_ok()
    );
    sql.execute(
        "UPDATE git_connections SET status='revoked' WHERE id=$1",
        &[&connection],
    )
    .unwrap();
    assert!(
        store
            .repository_binding(&reader, &project.id, &repository.id)
            .is_err()
    );
    sql.execute(
        "UPDATE git_connections SET status='active' WHERE id=$1",
        &[&connection],
    )
    .unwrap();
    assert!(
        store
            .repository_binding(&reader, &project.id, &repository.id)
            .is_ok()
    );

    assert!(matches!(
        store.repository_binding(&reader, "another-project", &repository.id),
        Err(PlatformError::NotFound)
    ));
    let metadata_key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Metadata only",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let metadata_reader = store.authenticate(&metadata_key.key).unwrap();
    assert!(matches!(
        store.repository_binding(&metadata_reader, &project.id, &repository.id),
        Err(PlatformError::NotFound)
    ));
    store.revoke_key(&ctx, &key.metadata.id).unwrap();
    assert!(matches!(
        store.repository_binding(&reader, &project.id, &repository.id),
        Err(PlatformError::Unauthorized)
    ));
    sql.execute("UPDATE project_repositories SET authorization_status='revoked',source_generation=source_generation+1,revision=revision+1 WHERE id=$1", &[&repository.id]).unwrap();
    assert!(
        store
            .repository_binding(&ctx, &project.id, &repository.id)
            .is_err()
    );
    sql.execute(
        "UPDATE project_repositories SET authorization_status='authorized' WHERE id=$1",
        &[&repository.id],
    )
    .unwrap();
    assert!(
        store
            .repository_binding(&ctx, &project.id, &repository.id)
            .is_err(),
        "restoring status must not revive a binding from an older source generation"
    );
    assert_eq!(
        sql.query_one(
            "SELECT snapshot_id FROM repository_bindings WHERE repository_id=$1",
            &[&repository.id]
        )
        .unwrap()
        .get::<_, String>(0),
        published.snapshot_id
    );
    std::fs::remove_dir_all(root).unwrap();
}
