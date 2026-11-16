#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL"]
fn eligible_destination_member_can_read_and_take_unclaimed_handoff() {
    let mut f = TaskPoolFixture::new(false);
    f.work
        .store
        .create_catalog_task_type(
            &f.work.ctx,
            &CatalogTaskTypeCreate {
                code: "handoff_work".into(),
                name: "Stage work".into(),
                purpose: TaskPurpose::Implementation,
                description: None,
                color: None,
                icon: TaskTypeIcon::default(),
            },
        )
        .unwrap();
    let task = f
        .work
        .store
        .create_pool_task(
            &f.work.ctx,
            &PoolTaskCreate {
                project_id: Some(f.work.project.id.clone()),
                definition: TaskCreate {
                    title: "Next stage".into(),
                    type_code: "handoff_work".into(),
                    entry_workflow_id: Some(f.version.workflow_id.clone()),
                    origin: "test".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    // Seed the post-delivery boundary; provider proof/handoff execution has separate tests.
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(f.work.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut db = postgres::Config::new()
        .host(&f.work.config.host)
        .port(f.work.config.postgres_port)
        .user(&f.work.config.postgres_user)
        .password(password.trim())
        .dbname(&f.work.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    db.batch_execute(&format!("SET search_path TO {}, public", f.work.namespace))
        .unwrap();
    db.execute("UPDATE tasks SET workflow_id=$2,workflow_version_id=$3,state_id='ready',lifecycle='active',assignee_id=NULL WHERE id=$1",&[&task.id,&f.version.workflow_id,&f.version.id]).unwrap();
    let developer = f.work.store.pool_task(&f.developer, &task.id).unwrap();
    let taken = f
        .work
        .store
        .take_task(
            &f.developer,
            &task.id,
            &TaskTake {
                expected_revision: developer.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    assert_eq!(taken.assignee_id.as_deref(), Some(f.developer.actor_id()));
}
