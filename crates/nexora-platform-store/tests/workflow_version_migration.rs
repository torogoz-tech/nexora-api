#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
#[path = "support/task_test_sql.rs"]
mod test_sql;
use nexora_platform::*;
use serde_json::json;
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL"]
fn migration_is_explicit_revisioned_authorized_and_does_not_run_actions() {
    let mut f = TaskPoolFixture::new(false);
    f.work
        .store
        .create_catalog_task_type(
            &f.work.ctx,
            &CatalogTaskTypeCreate {
                code: "impl_test".into(),
                name: "Implementation".into(),
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
            &f.manager,
            &PoolTaskCreate {
                project_id: None,
                definition: TaskCreate {
                    title: "Migrate me".into(),
                    type_code: "impl_test".into(),
                    entry_workflow_id: Some(f.version.workflow_id.clone()),
                    origin: "test".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let task = f
        .work
        .store
        .take_task(
            &f.developer,
            &task.id,
            &TaskTake {
                expected_revision: task.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    let flow = f
        .work
        .store
        .developer_workflow(&f.work.ctx, &f.version.workflow_id)
        .unwrap();
    let version = f
        .work
        .store
        .publish_developer_workflow(
            &f.work.ctx,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "New version".into(),
            },
        )
        .unwrap();
    assert_eq!(
        f.work
            .store
            .pool_task(&f.developer, &task.id)
            .unwrap()
            .workflow_version_id,
        Some(f.version.id.clone())
    );
    let input = TaskWorkflowMigration {
        expected_revision: task.revision,
        workflow_version_id: version.id.clone(),
        state_mapping: std::collections::BTreeMap::from([("ready".into(), "ready".into())]),
        reason: "Upgrade".into(),
    };
    assert!(matches!(
        f.work
            .store
            .migrate_pool_task_workflow(&f.developer, &task.id, &input),
        Err(PlatformError::Forbidden)
    ));
    let mut bad = input.clone();
    bad.state_mapping.insert("ready".into(), "done".into());
    assert!(matches!(
        f.work
            .store
            .migrate_pool_task_workflow(&f.manager, &task.id, &bad),
        Err(PlatformError::Conflict(_))
    ));
    let mut guard_sql = test_sql::connect(&f.work.config, &f.work.namespace);
    guard_sql
        .query_one(
            "SELECT pg_advisory_lock(hashtextextended($1,0))",
            &[&format!("task-transition:{}", task.id)],
        )
        .unwrap();
    assert!(matches!(
        f.work
            .store
            .migrate_pool_task_workflow(&f.manager, &task.id, &input),
        Err(PlatformError::Conflict(_))
    ));
    guard_sql
        .query_one(
            "SELECT pg_advisory_unlock(hashtextextended($1,0))",
            &[&format!("task-transition:{}", task.id)],
        )
        .unwrap();
    guard_sql.execute("INSERT INTO task_transition_runs(id,task_id,actor_id,credential_id,auth_method,task_revision,transition_id,fingerprint,parameters,record) VALUES('migration-wait',$1,$2,$3,'session',$4,'finish','test','{}','{\"status\":\"waiting_external\"}')",&[&task.id,&f.developer.actor_id(),&f.developer.credential_id(),&task.revision]).unwrap();
    assert!(matches!(
        f.work
            .store
            .migrate_pool_task_workflow(&f.manager, &task.id, &input),
        Err(PlatformError::Conflict(_))
    ));
    guard_sql
        .execute(
            "DELETE FROM task_transition_runs WHERE id='migration-wait'",
            &[],
        )
        .unwrap();
    let after = f
        .work
        .store
        .migrate_pool_task_workflow(&f.manager, &task.id, &input)
        .unwrap();
    assert_eq!(after.workflow_version_id, Some(version.id));
    assert_eq!(after.revision, task.revision + 1);
    assert!(
        f.work
            .store
            .migrate_pool_task_workflow(&f.manager, &task.id, &input)
            .is_err()
    );
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    let count:i64=sql.query_one("SELECT count(*) FROM task_history WHERE task_id=$1 AND operation='task.workflow_migrated'",&[&task.id]).unwrap().get(0);
    assert_eq!(count, 1);
    assert_eq!(
        f.work
            .store
            .task_transition_runs(&f.developer, &task.id)
            .unwrap()["items"],
        json!([])
    );
}
