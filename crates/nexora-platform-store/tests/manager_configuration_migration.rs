#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; upgrades existing managers without broadening operational scope"]
fn existing_managers_receive_global_configuration_grants() {
    let mut f = WorkItemFixture::new();
    let password = nexora_platform_store::SecretGenerator::token("migration_test").unwrap();
    let actor = f
        .store
        .create_user(&f.ctx, "existing_manager", &password, "Manager")
        .unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &actor.id,
            &MemberRoleChange {
                role: MemberRole::Manager,
                all_projects: false,
                project_ids: vec![f.project.id.clone()],
                expected_revision: 0,
            },
        )
        .unwrap();
    let db_password = zeroize::Zeroizing::new(
        std::fs::read_to_string(f.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&f.config.host)
        .port(f.config.postgres_port)
        .user(&f.config.postgres_user)
        .password(db_password.trim())
        .dbname(&f.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{}\"", f.namespace))
        .unwrap();
    sql.execute(
        "DELETE FROM actor_grants WHERE actor_id=$1 AND project_id IS NULL",
        &[&actor.id],
    )
    .unwrap();
    let before: i64 = sql
        .query_one(
            "SELECT count(*) FROM actor_grants WHERE actor_id=$1 AND project_id IS NOT NULL",
            &[&actor.id],
        )
        .unwrap()
        .get(0);
    sql.batch_execute(include_str!("../migrations/0066_manager_configuration.sql"))
        .unwrap();
    let grants = sql.query("SELECT capability FROM actor_grants WHERE actor_id=$1 AND project_id IS NULL ORDER BY capability", &[&actor.id]).unwrap();
    let actual: Vec<String> = grants.iter().map(|r| r.get(0)).collect();
    let mut expected: Vec<String> = MemberRole::Manager
        .configuration_capabilities()
        .iter()
        .map(|c| c.as_str().to_string())
        .collect();
    expected.sort();
    assert_eq!(actual, expected);
    let after: i64 = sql
        .query_one(
            "SELECT count(*) FROM actor_grants WHERE actor_id=$1 AND project_id IS NOT NULL",
            &[&actor.id],
        )
        .unwrap()
        .get(0);
    assert_eq!(before, after);
    let audit: i64 = sql.query_one("SELECT count(*) FROM audit_events WHERE resource_id=$1 AND operation='member.configuration_authority_migrated'", &[&actor.id]).unwrap().get(0);
    assert_eq!(audit, 1);
}
