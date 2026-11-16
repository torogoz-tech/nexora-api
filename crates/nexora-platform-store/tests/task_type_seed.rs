#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; seed preserves customized metadata and validates Lucide keys"]
fn seed_preserves_custom_types_and_adds_six_defaults() {
    let mut f = WorkItemFixture::new();
    let catalog = f.store.catalog_task_types(&f.ctx, None, 100).unwrap();
    for (code, icon) in [
        ("task", "list-todo"),
        ("bug", "bug"),
        ("improvement", "lightbulb"),
        ("research", "search"),
        ("maintenance", "wrench"),
        ("incident", "shield-alert"),
    ] {
        assert_eq!(
            catalog
                .items
                .iter()
                .find(|t| t.code == code)
                .unwrap()
                .icon
                .as_str(),
            icon
        )
    }
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
    let mut tx = sql.transaction().unwrap();
    let schema = format!("{}_seed", f.namespace);
    tx.batch_execute(&format!("CREATE SCHEMA \"{schema}\"; SET LOCAL search_path TO \"{schema}\"; CREATE TABLE task_type_catalog(code text PRIMARY KEY,name text NOT NULL,description text,color text,archived boolean DEFAULT false,revision bigint DEFAULT 1); INSERT INTO task_type_catalog VALUES('task','Mi tipo personalizado','Conservar descripción','#22C55E',true,17),('custom','Personalizado',NULL,NULL,false,2); ")).unwrap();
    tx.batch_execute(include_str!(
        "../migrations/0033_task_type_icons_and_seed.sql"
    ))
    .unwrap();
    let row=tx.query_one("SELECT name,description,color,archived,revision,icon FROM task_type_catalog WHERE code='task'",&[]).unwrap();
    assert_eq!(row.get::<_, String>(0), "Mi tipo personalizado");
    assert_eq!(row.get::<_, String>(1), "Conservar descripción");
    assert_eq!(row.get::<_, String>(2), "#22C55E");
    assert!(row.get::<_, bool>(3));
    assert_eq!(row.get::<_, i64>(4), 17);
    assert_eq!(row.get::<_, String>(5), "list-todo");
    assert_eq!(
        tx.query_one("SELECT count(*) FROM task_type_catalog", &[])
            .unwrap()
            .get::<_, i64>(0),
        7
    );
    assert!(
        tx.execute(
            "UPDATE task_type_catalog SET icon='arbitrary-svg' WHERE code='custom'",
            &[]
        )
        .is_err()
    );
    tx.rollback().unwrap();
    assert!(
        serde_json::from_value::<nexora_platform::CatalogTaskTypeCreate>(
            serde_json::json!({"code":"custom","name":"Test","icon":"unknown"})
        )
        .is_err()
    );
}
