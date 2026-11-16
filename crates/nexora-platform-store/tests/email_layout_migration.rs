#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::EmailTemplate;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; removes stock subject heading while preserving organization HTML"]
fn subject_heading_migration_only_updates_the_bundled_layout() {
    let mut f = WorkItemFixture::new();
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
    let migration = include_str!("../migrations/0039_email_layout_subject.sql");
    let original = migration.split("$nexora_layout$").nth(1).unwrap();
    sql.execute(
        "UPDATE installation_email_template SET html=$1,revision=10 WHERE singleton",
        &[&original],
    )
    .unwrap();
    sql.batch_execute(migration).unwrap();
    let migrated = f.store.email_template(&f.ctx).unwrap();
    assert_eq!(migrated.revision, 11);
    assert_eq!(migrated.html, EmailTemplate::DEFAULT_HTML);
    assert!(!migrated.html.contains("<h1"));
    let custom = "<h1>{{subject}}</h1><section>{{body}}</section>";
    sql.execute(
        "UPDATE installation_email_template SET html=$1,revision=30 WHERE singleton",
        &[&custom],
    )
    .unwrap();
    sql.batch_execute(migration).unwrap();
    let unchanged = f.store.email_template(&f.ctx).unwrap();
    assert_eq!(unchanged.revision, 30);
    assert_eq!(unchanged.html, custom);
    sql.execute(
        "UPDATE installation_email_template SET html=NULL,revision=40 WHERE singleton",
        &[],
    )
    .unwrap();
    sql.batch_execute(migration).unwrap();
    assert_eq!(f.store.email_template(&f.ctx).unwrap().revision, 41);
}
