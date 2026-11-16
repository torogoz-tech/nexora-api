#[path = "support/local_snapshot_fixture.rs"]
mod local_snapshot_fixture;
#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use local_snapshot_fixture::LocalSnapshotFixture;
use nexora_platform::LocalSnapshotAdoption;
use nexora_platform_store::PlatformStore;
use serde_json::json;

#[test]
#[ignore = "requires PostgreSQL/Dragonfly/Neo4j; failed audit atomically rolls back local catalog, source, publication and receipt"]
fn failed_attestation_audit_leaves_no_catalog_or_publication_and_retries_once() {
    let mut f = LocalSnapshotFixture::new();
    let project = f.work.project.id.clone();
    let operator = f.work.ctx.clone();
    let input: LocalSnapshotAdoption = serde_json::from_value(json!({
        "name":"Atomic import","root":f.root,"runtime_tenant_id":f.header.tenant_id.0,
        "snapshot_id":f.header.id.0,"expected_payload_hash":f.header.payload_hash,
        "reason":"Commit attestation and audit together"
    }))
    .unwrap();
    let canonical = json!({"project_id":project,"root_digest":f.header.repository_id.0,
        "runtime_tenant_id":input.runtime_tenant_id,"snapshot_id":input.snapshot_id,
        "expected_payload_hash":input.expected_payload_hash,"name":input.name,"reason":input.reason});
    let attempt = |store: &mut PlatformStore| {
        store.request(
            &operator,
            "local_snapshot.adopt",
            "atomic-adoption",
            &canonical,
            |store| store.adopt_local_snapshot(&operator, &project, &input),
        )
    };
    f.sql.batch_execute("CREATE FUNCTION reject_local_attestation_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='local_snapshot.adopted' THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='fixture attestation audit rejection'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_local_attestation_audit BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION reject_local_attestation_audit();").unwrap();
    assert!(attempt(&mut f.work.store).is_err());
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
        assert_eq!(count, 0, "partial adoption in {table}");
    }
    f.sql.batch_execute("DROP TRIGGER reject_local_attestation_audit ON audit_events; DROP FUNCTION reject_local_attestation_audit();").unwrap();
    let first = attempt(&mut f.work.store).unwrap();
    let replay = attempt(&mut f.work.store).unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(replay).unwrap()
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
        assert_eq!(count, 1, "duplicate adoption in {table}");
    }
    assert_eq!(
        f.runtime
            .query_header(&f.header.tenant_id, &f.header.repository_id, None)
            .unwrap()
            .payload_hash,
        f.header.payload_hash
    );
    std::fs::remove_dir_all(&f.root).unwrap();
}
