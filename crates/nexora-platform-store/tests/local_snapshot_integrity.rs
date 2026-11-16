#[path = "support/local_snapshot_fixture.rs"]
mod local_snapshot_fixture;
#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use local_snapshot_fixture::LocalSnapshotFixture;
use nexora_platform::LocalSnapshotAdoption;
use serde_json::json;

#[test]
#[ignore = "requires PostgreSQL/Dragonfly/Neo4j; reject wrong roots, hashes, tenants, corrupt payloads and projections without repair"]
fn local_attestation_requires_exact_verified_payload_and_projection() {
    let mut f = LocalSnapshotFixture::new();
    let project = f.work.project.id.clone();
    let operator = f.work.ctx.clone();
    let input: LocalSnapshotAdoption = serde_json::from_value(json!({
        "name":"Verified import","root":f.root,"runtime_tenant_id":f.header.tenant_id.0,
        "snapshot_id":f.header.id.0,"expected_payload_hash":f.header.payload_hash,"reason":"Verify exact persisted source"
    })).unwrap();
    let mut invalid = Vec::new();
    let mut value = input.clone();
    value.expected_payload_hash = "0".repeat(64);
    invalid.push(value);
    let mut value = input.clone();
    value.snapshot_id = "never-published".into();
    invalid.push(value);
    let mut value = input.clone();
    value.runtime_tenant_id = "foreign-tenant".into();
    invalid.push(value);
    let mut value = input.clone();
    value.root = f.root.join("source.js");
    invalid.push(value);
    let mut value = input.clone();
    value.root = f.root.parent().unwrap().to_owned();
    invalid.push(value);
    for (index, value) in invalid.into_iter().enumerate() {
        let canonical = json!({"case":index,"snapshot":value.snapshot_id,"expected_hash":value.expected_payload_hash});
        assert!(
            f.work
                .store
                .request(
                    &operator,
                    "local_snapshot.adopt",
                    &format!("invalid-{index}"),
                    &canonical,
                    |store| store.adopt_local_snapshot(&operator, &project, &value)
                )
                .is_err()
        );
    }
    let payload: Vec<u8> = f.sql.query_one("SELECT payload FROM nexora_runtime.snapshots WHERE tenant_id=$1 AND repository_id=$2 AND snapshot_id=$3",
        &[&f.header.tenant_id.0,&f.header.repository_id.0,&f.header.id.0]).unwrap().get(0);
    f.sql.execute("UPDATE nexora_runtime.snapshots SET payload=$4 WHERE tenant_id=$1 AND repository_id=$2 AND snapshot_id=$3",
        &[&f.header.tenant_id.0,&f.header.repository_id.0,&f.header.id.0,&&b"corrupt-fixture-payload"[..]]).unwrap();
    assert!(
        f.work
            .store
            .request(
                &operator,
                "local_snapshot.adopt",
                "corrupt-payload",
                &json!({"case":"payload"}),
                |store| store.adopt_local_snapshot(&operator, &project, &input)
            )
            .is_err()
    );
    f.sql.execute("UPDATE nexora_runtime.snapshots SET payload=$4 WHERE tenant_id=$1 AND repository_id=$2 AND snapshot_id=$3",
        &[&f.header.tenant_id.0,&f.header.repository_id.0,&f.header.id.0,&payload]).unwrap();
    let restored = f
        .runtime
        .snapshot(
            &f.header.tenant_id,
            &f.header.repository_id,
            Some(&f.header.id),
        )
        .unwrap()
        .unwrap();
    assert_eq!(restored.id, f.header.id);
    assert_eq!(
        f.projection(
            "MATCH (s:NexoraSnapshot {scope:$scope}) SET s.query_version=-1 RETURN s.query_version"
        ),
        json!(-1)
    );
    assert!(
        f.work
            .store
            .request(
                &operator,
                "local_snapshot.adopt",
                "corrupt-projection",
                &json!({"case":"projection"}),
                |store| store.adopt_local_snapshot(&operator, &project, &input)
            )
            .is_err()
    );
    assert_eq!(
        f.projection("MATCH (s:NexoraSnapshot {scope:$scope}) RETURN s.query_version"),
        json!(-1)
    );
    f.projection(&format!(
        "MATCH (s:NexoraSnapshot {{scope:$scope}}) SET s.query_version={} RETURN s.query_version",
        nexora_runtime::QuerySnapshot::VERSION
    ));
    let nodes = f.projection("MATCH (n:NexoraNode {scope:$scope}) RETURN count(n)");
    assert_eq!(nodes.as_u64(), Some(restored.graph.nodes.len() as u64));
    assert!(nodes.as_u64().unwrap() > 0);
    f.projection("MATCH (n:NexoraNode {scope:$scope}) WITH n LIMIT 1 DETACH DELETE n RETURN 1");
    assert!(
        f.work
            .store
            .request(
                &operator,
                "local_snapshot.adopt",
                "missing-projected-node",
                &json!({"case":"node-count"}),
                |store| store.adopt_local_snapshot(&operator, &project, &input)
            )
            .is_err()
    );
    assert_eq!(
        f.projection("MATCH (n:NexoraNode {scope:$scope}) RETURN count(n)")
            .as_u64()
            .unwrap(),
        nodes.as_u64().unwrap() - 1
    );
    for table in [
        "project_repositories",
        "local_repository_sources",
        "local_snapshot_publications",
        "request_receipts",
    ] {
        let count: i64 = f
            .sql
            .query_one(&format!("SELECT count(*) FROM {table}"), &[])
            .unwrap()
            .get(0);
        assert_eq!(count, 0, "invalid source left records in {table}");
    }
    std::fs::remove_dir_all(&f.root).unwrap();
}
