#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; uses and removes an isolated namespace"]
fn links_enforce_project_identity_uniqueness_and_soft_removal() {
    let mut f = WorkItemFixture::new();
    let task = f.task("Explicit PR evidence");
    let other = f.store.create_project(&f.ctx, "Other", None).unwrap();
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
    sql.execute("INSERT INTO project_repositories(id,project_id,provider,host,requested_full_name) VALUES('repo_pr_test',$1,'github','github.com','owner/repo')", &[&f.project.id]).unwrap();
    let insert = "INSERT INTO pull_request_links(id,project_id,repository_id,remote_id,pull_request_number,kind,task_id,created_by) VALUES($1,$2,'repo_pr_test','42',7,'task',$3,$4)";
    assert!(
        sql.execute(
            insert,
            &[&"cross_project", &other.id, &task.id, &f.ctx.actor_id()]
        )
        .is_err()
    );
    sql.execute(
        insert,
        &[&"link_first", &f.project.id, &task.id, &f.ctx.actor_id()],
    )
    .unwrap();
    assert!(
        sql.execute(
            insert,
            &[
                &"link_duplicate",
                &f.project.id,
                &task.id,
                &f.ctx.actor_id()
            ]
        )
        .is_err()
    );
    assert!(
        sql.execute(
            "UPDATE pull_request_links SET kind='requirement' WHERE id='link_first'",
            &[]
        )
        .is_err()
    );
    sql.execute(
        "UPDATE pull_request_links SET removed_at=now(),revision=revision+1 WHERE id='link_first'",
        &[],
    )
    .unwrap();
    sql.execute(
        insert,
        &[
            &"link_recreated",
            &f.project.id,
            &task.id,
            &f.ctx.actor_id(),
        ],
    )
    .unwrap();
    assert_eq!(
        sql.query_one(
            "SELECT count(*) FROM pull_request_links WHERE removed_at IS NULL",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        1
    );
    assert_eq!(
        sql.query_one("SELECT max(version) FROM platform_schema_versions", &[])
            .unwrap()
            .get::<_, i32>(0),
        28
    );
    drop(f.store);
    sql.batch_execute(&format!("DROP SCHEMA \"{}\" CASCADE", f.namespace))
        .unwrap();
    std::fs::remove_file(
        f.config
            .secrets_directory
            .join(format!("{}_master_key_v1", f.namespace)),
    )
    .unwrap();
}
