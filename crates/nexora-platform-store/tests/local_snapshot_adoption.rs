#[path = "support/local_snapshot_fixture.rs"]
mod local_snapshot_fixture;
#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use local_snapshot_fixture::LocalSnapshotFixture;
use nexora_platform::*;
use nexora_platform_store::PlatformStore;
use serde_json::{Value, json};

#[test]
#[ignore = "requires PostgreSQL/Dragonfly/Neo4j; authenticated local attestation preserves runtime identity and existing knowledge"]
fn operator_adoption_is_scoped_idempotent_and_does_not_republish_runtime_data() {
    let mut f = LocalSnapshotFixture::new();
    let project = f.work.project.id.clone();
    let operator = f.work.ctx.clone();
    let input: LocalSnapshotAdoption = serde_json::from_value(json!({
        "name":"Imported fixture","root":f.root,"runtime_tenant_id":f.header.tenant_id.0,
        "snapshot_id":f.header.id.0,"expected_payload_hash":f.header.payload_hash,
        "reason":"Adopt existing authorized local context without reindexing"
    }))
    .unwrap();
    let canonical = json!({"project_id":project,"name":input.name,
        "root_digest":f.header.repository_id.0,"runtime_tenant_id":input.runtime_tenant_id,
        "snapshot_id":input.snapshot_id,"expected_payload_hash":input.expected_payload_hash,
        "reason":input.reason,"repository_id":null,"expected_revision":null});
    let original = serde_json::to_value(
        f.runtime
            .snapshot(
                &f.header.tenant_id,
                &f.header.repository_id,
                Some(&f.header.id),
            )
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let before: i64 = f.sql.query_one("SELECT count(*) FROM nexora_runtime.knowledge_revisions WHERE tenant_id=$1 AND repository_id=$2",
        &[&f.header.tenant_id.0,&f.header.repository_id.0]).unwrap().get(0);
    assert_eq!(before, 1);
    let adopt = |store: &mut PlatformStore| {
        store.request(
            &operator,
            "local_snapshot.adopt",
            "adopt-once",
            &canonical,
            |store| store.adopt_local_snapshot(&operator, &project, &input),
        )
    };
    let adopted = adopt(&mut f.work.store).unwrap();
    let replay = adopt(&mut f.work.store).unwrap();
    assert_eq!(
        serde_json::to_value(&adopted).unwrap(),
        serde_json::to_value(&replay).unwrap()
    );
    assert_eq!(adopted.runtime_tenant_id, f.header.tenant_id.0);
    assert_eq!(adopted.runtime_repository_id, f.header.repository_id.0);
    assert_eq!(adopted.snapshot_id, f.header.id.0);
    assert_eq!(adopted.payload_hash, f.header.payload_hash);
    assert_ne!(adopted.repository_id, adopted.runtime_repository_id);
    assert_eq!(adopted.source_revision, 1);
    let metadata = f
        .work
        .store
        .get_repository(&operator, &project, &adopted.repository_id)
        .unwrap();
    assert_eq!(metadata.source_kind, RepositorySourceKind::LocalSnapshot);
    assert!(metadata.provider.is_none());
    assert!(metadata.connection_id.is_none());
    assert!(metadata.latest_source_commit.is_none());
    assert_eq!(
        metadata.local_source.unwrap().publication_id,
        adopted.publication_id
    );
    let preserved = f
        .runtime
        .snapshot(&f.header.tenant_id, &f.header.repository_id, None)
        .unwrap()
        .unwrap();
    assert_eq!(serde_json::to_value(preserved).unwrap(), original);
    let after: i64 = f.sql.query_one("SELECT count(*) FROM nexora_runtime.knowledge_revisions WHERE tenant_id=$1 AND repository_id=$2",
        &[&f.header.tenant_id.0,&f.header.repository_id.0]).unwrap().get(0);
    assert_eq!(after, before);
    let publications: i64 = f
        .sql
        .query_one(
            "SELECT count(*) FROM local_snapshot_publications WHERE source_id=$1",
            &[&adopted.source_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(publications, 1);
    let git_bindings: i64 = f
        .sql
        .query_one(
            "SELECT count(*) FROM repository_bindings WHERE repository_id=$1",
            &[&adopted.repository_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(git_bindings, 0);
    let scoped_key = f
        .work
        .store
        .issue_key(
            &operator,
            operator.actor_id(),
            "Human key cannot attest",
            KeyScope {
                capabilities: vec![
                    Capability::InstallationManage,
                    Capability::RepositoriesWrite,
                ],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let keyed_human = f.work.store.authenticate(&scoped_key.key).unwrap();
    assert!(matches!(
        f.work.store.request(
            &keyed_human,
            "local_snapshot.adopt",
            "key-cannot-attest",
            &canonical,
            |store| store.adopt_local_snapshot(&keyed_human, &project, &input)
        ),
        Err(PlatformError::Forbidden)
    ));
    let other = f
        .work
        .store
        .create_project(&operator, "Other project", None)
        .unwrap();
    let mut another = canonical.clone();
    another["project_id"] = json!(other.id);
    assert!(
        f.work
            .store
            .request(
                &operator,
                "local_snapshot.adopt",
                "no-project-alias",
                &another,
                |store| store.adopt_local_snapshot(&operator, &other.id, &input)
            )
            .is_err()
    );
    let audit: Value = f.sql.query_one("SELECT metadata FROM audit_events WHERE operation='local_snapshot.adopted' AND resource_id=$1",
        &[&adopted.repository_id]).unwrap().get(0);
    assert!(audit.get("root").is_none());
    assert!(!audit.to_string().contains(f.root.to_str().unwrap()));
    std::fs::remove_dir_all(&f.root).unwrap();
}
