#[path = "support/local_snapshot_fixture.rs"]
mod local_snapshot_fixture;
#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use local_snapshot_fixture::LocalSnapshotFixture;
use nexora_platform::*;
use nexora_platform_store::PlatformStore;
use serde_json::json;
use sha2::{Digest, Sha256};

#[test]
#[ignore = "requires local databases; count-preserving projection corruption must fail adoption and reauthorization without repair"]
fn attestation_compares_projected_records_and_endpoints_with_the_validated_snapshot() {
    let mut f = LocalSnapshotFixture::new();
    let project = f.work.project.id.clone();
    let ctx = f.work.ctx.clone();
    let input: LocalSnapshotAdoption = serde_json::from_value(json!({"name":"Integrity fixture","root":f.root,
        "runtime_tenant_id":f.header.tenant_id.0,"snapshot_id":f.header.id.0,"expected_payload_hash":f.header.payload_hash,
        "reason":"Require exact persisted projection integrity"})).unwrap();
    let canonical = json!({"project_id":project,"root_digest":f.header.repository_id.0,"snapshot":input.snapshot_id,"hash":input.expected_payload_hash});
    let attempt = |store: &mut PlatformStore, id: &str| {
        store.request(&ctx, "local_snapshot.adopt", id, &canonical, |store| {
            store.adopt_local_snapshot(&ctx, &project, &input)
        })
    };
    let stored = f
        .runtime
        .snapshot(
            &f.header.tenant_id,
            &f.header.repository_id,
            Some(&f.header.id),
        )
        .unwrap()
        .unwrap();
    assert_eq!(f.projection("MATCH (n:NexoraNode {scope:$scope,name:'retained_identity'}) SET n.name_hash='count-preserving-corruption' RETURN count(n)"),json!(1));
    assert_eq!(
        f.projection("MATCH (n:NexoraNode {scope:$scope}) RETURN count(n)")
            .as_u64(),
        Some(stored.graph.nodes.len() as u64)
    );
    assert!(
        attempt(&mut f.work.store, "corrupt-node-query-property").is_err(),
        "preexisting query-property corruption must reject attestation even when counts and marker match"
    );
    assert_eq!(
        f.projection(
            "MATCH (n:NexoraNode {scope:$scope,name:'retained_identity'}) RETURN n.name_hash"
        ),
        json!("count-preserving-corruption")
    );
    let hash = format!("{:x}", Sha256::digest(b"retained_identity"));
    f.projection(&format!("MATCH (n:NexoraNode {{scope:$scope,name:'retained_identity'}}) SET n.name_hash='{hash}' RETURN count(n)"));
    f.projection("MATCH (n:NexoraNode {scope:$scope,name:'retained_identity'}) SET n.payload=n.payload+' ' RETURN count(n)");
    assert!(attempt(&mut f.work.store, "corrupt-node-payload").is_err());
    f.projection("MATCH (n:NexoraNode {scope:$scope,name:'retained_identity'}) SET n.payload=substring(n.payload,0,size(n.payload)-1) RETURN count(n)");
    f.projection("MATCH (n:NexoraNode {scope:$scope,name:'retained_identity'}) SET n.ordinal=n.ordinal+1 RETURN count(n)");
    assert!(attempt(&mut f.work.store, "corrupt-node-ordinal").is_err());
    f.projection("MATCH (n:NexoraNode {scope:$scope,name:'retained_identity'}) SET n.ordinal=n.ordinal-1 RETURN count(n)");
    let resolution=f.projection("MATCH ()-[e:NEXORA_EDGE {scope:$scope}]->() WITH e ORDER BY e.ordinal LIMIT 1 RETURN e.resolution");
    f.projection("MATCH ()-[e:NEXORA_EDGE {scope:$scope}]->() WITH e ORDER BY e.ordinal LIMIT 1 SET e.resolution='CountPreservingCorruption' RETURN count(e)");
    assert!(attempt(&mut f.work.store, "corrupt-edge-query-property").is_err());
    f.projection(&format!("MATCH ()-[e:NEXORA_EDGE {{scope:$scope}}]->() WITH e ORDER BY e.ordinal LIMIT 1 SET e.resolution={resolution} RETURN count(e)"));
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
        assert_eq!(count, 0);
    }
    let audit:i64=f.sql.query_one("SELECT count(*) FROM audit_events WHERE operation IN ('local_snapshot.adopted','local_source.reauthorized')",&[]).unwrap().get(0);
    assert_eq!(audit, 0);
    let healthy = attempt(&mut f.work.store, "healthy-projection").unwrap();
    let change = LocalSourceChange {
        expected_revision: healthy.source_revision,
        reason: "Require a new attestation before restoring trust".into(),
    };
    let revoked = f
        .work
        .store
        .request(
            &ctx,
            "local_source.revoke",
            "revoke-integrity",
            &json!({"repository_id":healthy.repository_id,"change":change}),
            |store| store.revoke_local_source(&ctx, &project, &healthy.repository_id, &change),
        )
        .unwrap();
    assert_eq!(f.projection("MATCH (a:NexoraNode {scope:$scope})-[e:NEXORA_EDGE {scope:$scope}]->(b:NexoraNode {scope:$scope}) WHERE a<>b WITH a,e ORDER BY e.ordinal LIMIT 1 CREATE (a)-[bad:NEXORA_EDGE]->(a) SET bad=properties(e) DELETE e RETURN count(bad)"),json!(1));
    assert_eq!(
        f.projection("MATCH ()-[e:NEXORA_EDGE {scope:$scope}]->() RETURN count(e)")
            .as_u64(),
        Some(stored.graph.edges.len() as u64)
    );
    let mut restore = input.clone();
    restore.repository_id = Some(healthy.repository_id.clone());
    restore.expected_revision = Some(revoked.source_revision);
    assert!(f.work.store.request(&ctx,"local_source.reauthorize","corrupt-edge-endpoint",&json!({"repository_id":healthy.repository_id,"expected_revision":revoked.source_revision}),
        |store| store.reauthorize_local_source(&ctx,&project,&restore)).is_err());
    let metadata = f
        .work
        .store
        .get_repository(&ctx, &project, &healthy.repository_id)
        .unwrap()
        .local_source
        .unwrap();
    assert_eq!(metadata.state, LocalSourceState::Revoked);
    assert_eq!(metadata.source_revision, revoked.source_revision);
    let count: i64 = f
        .sql
        .query_one(
            "SELECT count(*) FROM local_snapshot_publications WHERE source_id=$1",
            &[&healthy.source_id],
        )
        .unwrap()
        .get(0);
    assert_eq!(count, 1);
    let audit: i64 = f
        .sql
        .query_one(
            "SELECT count(*) FROM audit_events WHERE operation='local_source.reauthorized'",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(audit, 0);
    std::fs::remove_dir_all(&f.root).unwrap();
}
