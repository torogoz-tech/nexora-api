use nexora_platform::{
    GitProvider, GitRepositoryAuthorization, PlatformError, RepositoryJobKind,
    RepositoryJobRequest, RepositoryRegistration, RepositorySourceKind,
};
use nexora_platform_store::{GitNetworkPolicy, PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use std::path::Path;
use zeroize::Zeroizing;

#[test]
#[ignore = "requires PostgreSQL; isolated local-source schema, conditional Git constraints and immutable identities"]
fn local_source_schema_preserves_git_contracts_and_separates_publication_authority() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = SecretGenerator::id("nexora_platform_local").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    store
        .bootstrap("Local source fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let project = store.create_project(&ctx, "Sources", None).unwrap();
    let git = store
        .register_repository(
            &ctx,
            &project.id,
            &RepositoryRegistration {
                provider: GitProvider::Github,
                full_name: "fixture/source".into(),
            },
        )
        .unwrap();
    let password = Zeroizing::new(
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{namespace}\" "))
        .unwrap();
    let version: i32 = sql
        .query_one("SELECT max(version) FROM platform_schema_versions", &[])
        .unwrap()
        .get(0);
    assert_eq!(version, 23);
    let before = serde_json::to_value(&git).unwrap();
    store.initialize().unwrap();
    assert_eq!(
        serde_json::to_value(store.get_repository(&ctx, &project.id, &git.id).unwrap()).unwrap(),
        before
    );
    assert!(
        sql.execute(
            "UPDATE project_repositories SET provider=NULL WHERE id=$1",
            &[&git.id]
        )
        .is_err()
    );
    assert!(
        sql.execute(
            "UPDATE project_repositories SET authorization_status='authorized' WHERE id=$1",
            &[&git.id]
        )
        .is_err()
    );
    assert!(sql.execute("UPDATE project_repositories SET source_kind='local_snapshot',provider=NULL,host=NULL,requested_full_name=NULL,local_name='Forbidden conversion' WHERE id=$1", &[&git.id]).is_err());
    let local = SecretGenerator::id("repo").unwrap();
    sql.execute("INSERT INTO project_repositories(id,project_id,source_kind,local_name,authorization_status) VALUES($1,$2,'local_snapshot','Imported source','authorized')", &[&local,&project.id]).unwrap();
    assert!(
        sql.execute(
            "UPDATE project_repositories SET provider='github' WHERE id=$1",
            &[&local]
        )
        .is_err()
    );
    let source = SecretGenerator::id("lsrc").unwrap();
    let publication = SecretGenerator::id("lpub").unwrap();
    let runtime_repository = "a".repeat(64);
    let hash = "b".repeat(64);
    let mut tx = sql.transaction().unwrap();
    tx.execute("INSERT INTO local_repository_sources(id,repository_id,project_id,runtime_tenant_id,runtime_repository_id,owner_actor_id,current_publication_id) VALUES($1,$2,$3,'legacy-tenant',$4,$5,$6)", &[&source,&local,&project.id,&runtime_repository,&ctx.actor_id(),&publication]).unwrap();
    tx.execute("INSERT INTO local_snapshot_publications(id,source_id,authorization_revision,snapshot_id,payload_hash,binding_revision,attested_by,reason) VALUES($1,$2,1,'legacy-snapshot',$3,1,$4,'Fixture attestation')", &[&publication,&source,&hash,&ctx.actor_id()]).unwrap();
    tx.commit().unwrap();
    let metadata = store.get_repository(&ctx, &project.id, &local).unwrap();
    assert_eq!(metadata.source_kind, RepositorySourceKind::LocalSnapshot);
    assert_eq!(metadata.local_source.as_ref().unwrap().source_id, source);
    let public = serde_json::to_value(&metadata).unwrap();
    assert!(public.get("runtime_tenant_id").is_none());
    assert!(
        public["local_source"]
            .get("runtime_repository_id")
            .is_none()
    );
    assert!(matches!(
        store.enqueue_repository_job(
            &ctx,
            &project.id,
            &local,
            RepositoryJobKind::Index,
            &RepositoryJobRequest {
                expected_revision: 1
            }
        ),
        Err(PlatformError::Validation(
            "local snapshots do not support Git jobs"
        ))
    ));
    assert!(matches!(
        store.authorize_repository(
            &ctx,
            &project.id,
            &local,
            &GitRepositoryAuthorization {
                connection_id: "not-a-connection".into(),
                expected_revision: 1,
                expected_remote_id: None
            },
            &GitNetworkPolicy::default()
        ),
        Err(PlatformError::Validation(
            "local snapshots do not support Git authorization"
        ))
    ));
    assert!(
        sql.execute(
            "UPDATE local_snapshot_publications SET payload_hash=$2 WHERE id=$1",
            &[&publication, &"c".repeat(64)]
        )
        .is_err()
    );
    assert!(
        sql.execute(
            "DELETE FROM local_snapshot_publications WHERE id=$1",
            &[&publication]
        )
        .is_err()
    );
    assert!(
        sql.execute(
            "UPDATE local_repository_sources SET runtime_repository_id=$2,revision=revision+1 WHERE id=$1",
            &[&source, &"d".repeat(64)]
        )
        .is_err()
    );
    assert!(sql.execute("INSERT INTO repository_bindings(repository_id,project_id,runtime_tenant_id,runtime_repository_id,snapshot_id,payload_hash,pinned_commit,source_generation,repository_revision,connection_id,connection_revision,job_id,attempt,completion_digest) VALUES($1,$2,'legacy-tenant',$3,'legacy-snapshot',$4,NULL,0,1,'no-connection',1,'no-job',1,$5)", &[&local,&project.id,&runtime_repository,&hash,&&[0u8;32][..]]).is_err());
    let second = SecretGenerator::id("repo").unwrap();
    sql.execute("INSERT INTO project_repositories(id,project_id,source_kind,local_name) VALUES($1,$2,'local_snapshot','Duplicate scope')", &[&second,&project.id]).unwrap();
    let duplicate = sql.execute("INSERT INTO local_repository_sources(id,repository_id,project_id,runtime_tenant_id,runtime_repository_id,owner_actor_id,current_publication_id) VALUES($1,$2,$3,'legacy-tenant',$4,$5,$6)", &[&SecretGenerator::id("lsrc").unwrap(),&second,&project.id,&runtime_repository,&ctx.actor_id(),&publication]).unwrap_err();
    assert_eq!(
        duplicate.code(),
        Some(&postgres::error::SqlState::UNIQUE_VIOLATION)
    );
}
