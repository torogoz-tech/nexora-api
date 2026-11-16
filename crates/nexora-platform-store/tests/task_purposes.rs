#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use task_pool_fixture::TaskPoolFixture;

#[test]
#[ignore = "requires PostgreSQL"]
fn general_tasks_start_without_workflow_and_reject_dual_primary_links() {
    let mut f = TaskPoolFixture::new(false);
    let task = f.create(None, None);
    assert_eq!(task.purpose, TaskPurpose::General);
    assert!(task.governed_stage);
    let taken = f
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
    assert_eq!(taken.lifecycle, TaskLifecycle::Active);
    assert!(taken.workflow_id.is_none());
    assert!(taken.workflow_version_id.is_none());
    let rejected = f.work.store.create_pool_task(
        &f.work.ctx,
        &PoolTaskCreate {
            project_id: None,
            definition: TaskCreate {
                title: "Invalid primary links".into(),
                type_code: "task".into(),
                origin: "test".into(),
                primary_requirement_id: Some("requirement".into()),
                primary_feature_id: Some("feature".into()),
                ..Default::default()
            },
        },
    );
    assert!(matches!(rejected, Err(PlatformError::Validation(_))));
    f.work
        .store
        .create_catalog_task_type(
            &f.work.ctx,
            &CatalogTaskTypeCreate {
                purpose: TaskPurpose::ProductDesign,
                code: "design_draft".into(),
                name: "Design draft".into(),
                description: None,
                color: None,
                icon: TaskTypeIcon::default(),
            },
        )
        .unwrap();
    let draft = f
        .work
        .store
        .create_pool_task(
            &f.work.ctx,
            &PoolTaskCreate {
                project_id: None,
                definition: TaskCreate {
                    title: "Requirement not selected yet".into(),
                    type_code: "design_draft".into(),
                    origin: "test".into(),
                    entry_workflow_id: Some(f.version.workflow_id.clone()),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    assert!(matches!(
        f.work.store.take_task(
            &f.developer,
            &draft.id,
            &TaskTake {
                expected_revision: draft.revision,
                workflow_id: None
            },
            None
        ),
        Err(PlatformError::Validation(
            "product design requires a primary requirement before taking the task"
        ))
    ));
}

#[test]
#[ignore = "requires PostgreSQL"]
fn workflow_is_selected_on_task_and_assignee_must_be_eligible() {
    let mut f = TaskPoolFixture::new(false);
    f.work
        .store
        .create_catalog_task_type(
            &f.work.ctx,
            &CatalogTaskTypeCreate {
                code: "delivery_work".into(),
                name: "Delivery".into(),
                purpose: TaskPurpose::Implementation,
                description: None,
                color: None,
                icon: TaskTypeIcon::default(),
            },
        )
        .unwrap();
    let input = TaskCreate {
        title: "Implement checkout".into(),
        type_code: "delivery_work".into(),
        origin: "test".into(),
        entry_workflow_id: Some(f.version.workflow_id.clone()),
        assignee_id: Some(f.developer.actor_id().into()),
        ..Default::default()
    };
    let task = f
        .work
        .store
        .create_pool_task(
            &f.work.ctx,
            &PoolTaskCreate {
                project_id: None,
                definition: input.clone(),
            },
        )
        .unwrap();
    assert_eq!(task.purpose, TaskPurpose::Implementation);
    assert_eq!(task.entry_workflow_id, Some(f.version.workflow_id.clone()));
    assert!(
        f.work
            .store
            .take_task(
                &f.developer,
                &task.id,
                &TaskTake {
                    expected_revision: task.revision,
                    workflow_id: Some("arbitrary_other_workflow".into())
                },
                None
            )
            .is_err()
    );
    let taken = f
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
    assert_eq!(taken.workflow_id, Some(f.version.workflow_id.clone()));
    assert_eq!(taken.workflow_version_id, Some(f.version.id.clone()));
    // Simulate the persisted handoff state, then update member eligibility to a newer published graph.
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(f.work.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&f.work.config.host)
        .port(f.work.config.postgres_port)
        .user(&f.work.config.postgres_user)
        .password(password.trim())
        .dbname(&f.work.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{}\"", f.work.namespace))
        .unwrap();
    sql.execute(
        "UPDATE tasks SET assignee_id=NULL,revision=revision+1 WHERE id=$1",
        &[&task.id],
    )
    .unwrap();
    let workflow = f
        .work
        .store
        .developer_workflow(&f.work.ctx, &f.version.workflow_id)
        .unwrap();
    let newer = f
        .work
        .store
        .publish_developer_workflow(
            &f.work.ctx,
            &workflow.id,
            &WorkflowPublish {
                expected_revision: workflow.revision,
                reason: "New published revision".into(),
            },
        )
        .unwrap();
    let membership = f
        .work
        .store
        .actor_workflows(&f.work.ctx, f.developer.actor_id())
        .unwrap()
        .into_iter()
        .find(|item| item.workflow_id == workflow.id)
        .unwrap();
    f.work
        .store
        .assign_actor_workflow(
            &f.work.ctx,
            f.developer.actor_id(),
            &DeveloperWorkflowChange {
                expected_revision: membership.revision,
                workflow_version_id: newer.id.clone(),
                reason: "New member revision".into(),
            },
        )
        .unwrap();
    let handed = f.work.store.pool_task(&f.work.ctx, &task.id).unwrap();
    let resumed = f
        .work
        .store
        .take_task(
            &f.developer,
            &task.id,
            &TaskTake {
                expected_revision: handed.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    assert_eq!(resumed.workflow_version_id, Some(f.version.id.clone()));
    assert_ne!(resumed.workflow_version_id, Some(newer.id));
    let incompatible = TaskCreate {
        assignee_id: Some(f.manager.actor_id().into()),
        ..input
    };
    assert!(
        f.work
            .store
            .create_pool_task(
                &f.work.ctx,
                &PoolTaskCreate {
                    project_id: None,
                    definition: incompatible
                }
            )
            .is_err()
    );
}
