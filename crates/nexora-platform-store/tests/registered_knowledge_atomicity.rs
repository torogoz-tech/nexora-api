#[path = "support/published_repository_fixture.rs"]
mod published_repository_fixture;
use nexora_core::{RepositoryId, SnapshotId, TenantId};
use nexora_platform::*;
use nexora_platform_store::PlatformStore;
use nexora_runtime::KnowledgeCommand;
use published_repository_fixture::PublishedRepositoryFixture;
use serde_json::{Value, json};
#[test]
#[ignore = "requires PostgreSQL/Dragonfly/Neo4j; failed platform audit rolls back runtime ledger and receipt in the same transaction"]
fn a_knowledge_write_and_its_platform_receipt_commit_or_roll_back_together() {
    let mut f = PublishedRepositoryFixture::new();
    let writer = f.contributor("Atomic writer", true);
    let project = f.work.project.id.clone();
    let repository = f.binding.repository_id.clone();
    let original = f.binding.clone();
    let command:KnowledgeCommand=serde_json::from_value(json!({"operation":"contribute","request_id":"atomic-once","draft":{"kind":"decision","statement":"The audit and ledger must commit together.","evidence":[{"path":"source.js","start_line":1,"line_count":1}]}})).unwrap();
    let canonical = json!({"project_id":project,"repository_id":repository,"snapshot":Value::Null,"command":command});
    let attempt = |store: &mut PlatformStore| {
        store.request(
            &writer,
            "knowledge.contribute",
            "atomic-once",
            &canonical,
            |store| {
                store.query_repository_knowledge(&writer, &project, &repository, None, &command)
            },
        )
    };
    f.sql.batch_execute("CREATE FUNCTION reject_knowledge_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation LIKE 'knowledge.%' THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='fixture audit rejection'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_knowledge_audit BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION reject_knowledge_audit();").unwrap();
    assert!(attempt(&mut f.work.store).is_err());
    let list = KnowledgeCommand::List {
        status: None,
        node: None,
        offset: 0,
        limit: 10,
    };
    let actual = f
        .runtime
        .knowledge(
            &TenantId(original.runtime_tenant_id.clone()),
            &RepositoryId(original.runtime_repository_id.clone()),
            Some(&SnapshotId(original.snapshot_id.clone())),
            "fixture-inspector",
            &list,
        )
        .unwrap();
    assert_eq!(actual["total"], 0);
    let receipts:i64=f.sql.query_one("SELECT count(*) FROM request_receipts WHERE actor_id=$1 AND operation='knowledge.contribute' AND request_id='atomic-once'",&[&writer.actor_id()]).unwrap().get(0);
    assert_eq!(receipts, 0);
    f.sql.batch_execute("DROP TRIGGER reject_knowledge_audit ON audit_events; DROP FUNCTION reject_knowledge_audit();").unwrap();
    let written = attempt(&mut f.work.store).unwrap();
    assert_eq!(written["contribution"]["revision"], 1);
    assert_eq!(written["contribution"]["actor"], writer.actor_id());
    let actual = f
        .runtime
        .knowledge(
            &TenantId(original.runtime_tenant_id.clone()),
            &RepositoryId(original.runtime_repository_id.clone()),
            Some(&SnapshotId(original.snapshot_id.clone())),
            "fixture-inspector",
            &list,
        )
        .unwrap();
    assert_eq!(actual["total"], 1);
    let rows:i64=f.sql.query_one("SELECT count(*) FROM nexora_runtime.knowledge_revisions WHERE tenant_id=$1 AND repository_id=$2",&[&original.runtime_tenant_id,&original.runtime_repository_id]).unwrap().get(0);
    assert_eq!(rows, 1);
    let next = f.publish_revision(2);
    assert_ne!(next.snapshot_id, original.snapshot_id);
    assert_eq!(attempt(&mut f.work.store).unwrap(), written);
    f.sql
        .execute(
            "UPDATE git_connections SET status='revoked',revision=revision+1 WHERE id=$1",
            &[&f.connection],
        )
        .unwrap();
    assert!(matches!(
        attempt(&mut f.work.store),
        Err(PlatformError::NotFound)
    ));
    let rows:i64=f.sql.query_one("SELECT count(*) FROM nexora_runtime.knowledge_revisions WHERE tenant_id=$1 AND repository_id=$2",&[&original.runtime_tenant_id,&original.runtime_repository_id]).unwrap().get(0);
    assert_eq!(rows, 1);
    std::fs::remove_dir_all(&f.root).unwrap();
}
