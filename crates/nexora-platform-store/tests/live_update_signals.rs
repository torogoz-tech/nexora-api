#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; durable transactional MCP knowledge invalidations"]
fn knowledge_signals_are_atomic_persistent_and_project_scoped() {
    let mut f = WorkItemFixture::new();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Knowledge SSE",
            KeyScope {
                capabilities: vec![Capability::KnowledgeRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(f.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&f.config.host)
        .port(f.config.postgres_port)
        .user(&f.config.postgres_user)
        .password(password.trim())
        .dbname(&f.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{}\"", f.namespace))
        .unwrap();
    let snapshot = f
        .store
        .live_update_snapshot(&key.key, Some(&f.project.id))
        .unwrap();
    assert_eq!(snapshot["knowledge"], 0);
    let mut tx = sql.transaction().unwrap();
    tx.execute("INSERT INTO audit_events(id,operation,metadata) VALUES('live_rollback','knowledge.contribute',$1)",&[&json!({"project_id":f.project.id})]).unwrap();
    assert_eq!(
        f.store
            .live_update_snapshot(&key.key, Some(&f.project.id))
            .unwrap()["knowledge"],
        0
    );
    tx.rollback().unwrap();
    sql.execute("INSERT INTO audit_events(id,operation,metadata) VALUES('live_committed','knowledge.contribute',$1)",&[&json!({"project_id":f.project.id,"knowledge_id":"not-delivered"})]).unwrap();
    let updated = f
        .store
        .live_update_snapshot(&key.key, Some(&f.project.id))
        .unwrap();
    assert_eq!(updated["knowledge"], 1);
    assert_eq!(updated.len(), 1);
    sql.execute("INSERT INTO audit_events(id,operation,metadata) VALUES('live_other','knowledge.review',$1)",&[&json!({"project_id":"other-project"})]).unwrap();
    assert_eq!(
        f.store
            .live_update_snapshot(&key.key, Some(&f.project.id))
            .unwrap()["knowledge"],
        1
    );
    let mut other = nexora_platform_store::PlatformStore::connect(&f.config, &f.namespace).unwrap();
    assert_eq!(
        other
            .live_update_snapshot(&key.key, Some(&f.project.id))
            .unwrap()["knowledge"],
        1
    );
}
