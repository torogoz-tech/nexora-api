#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL"]
fn terminal_completes_stage_but_not_publication_and_children_use_stage_completion() {
    let mut f = TaskPoolFixture::new(false);
    f.work
        .store
        .create_catalog_task_type(
            &f.work.ctx,
            &CatalogTaskTypeCreate {
                code: "stage_work".into(),
                name: "Stage work".into(),
                purpose: TaskPurpose::Implementation,
                description: None,
                color: None,
                icon: TaskTypeIcon::default(),
            },
        )
        .unwrap();
    let parent = f
        .work
        .store
        .create_pool_task(
            &f.work.ctx,
            &PoolTaskCreate {
                project_id: None,
                definition: TaskCreate {
                    title: "Parent".into(),
                    type_code: "stage_work".into(),
                    entry_workflow_id: Some(f.version.workflow_id.clone()),
                    origin: "test".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let parent = f
        .work
        .store
        .take_task(
            &f.developer,
            &parent.id,
            &TaskTake {
                expected_revision: parent.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    let child = f
        .work
        .store
        .create_task_subtask(
            &f.developer,
            &parent.id,
            &TaskSubtaskCreate {
                expected_revision: parent.revision,
                work_kind: TaskSubtaskKind::Decomposition,
                tested_commit_sha: None,
                tested_repository_id: None,
                definition: TaskCreate {
                    title: "Child".into(),
                    type_code: "stage_work".into(),
                    entry_workflow_id: Some(f.version.workflow_id.clone()),
                    origin: "test".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let parent = f.work.store.pool_task(&f.developer, &parent.id).unwrap();
    let request = |revision| TaskTransitionRequest {
        expected_revision: revision,
        ..Default::default()
    };
    assert!(
        f.work
            .store
            .transition_pool_task(
                &f.developer,
                &parent.id,
                "finish",
                &request(parent.revision)
            )
            .is_err()
    );
    let child = f
        .work
        .store
        .take_task(
            &f.developer,
            &child.id,
            &TaskTake {
                expected_revision: child.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    let child = f
        .work
        .store
        .transition_pool_task(&f.developer, &child.id, "finish", &request(child.revision))
        .unwrap();
    assert_eq!(child.lifecycle, TaskLifecycle::Active);
    let board_query = VisibleTaskQuery {
        workflow_id: Some(f.version.workflow_id.clone()),
        lifecycle: Some(TaskLifecycle::Active),
        ..Default::default()
    };
    assert!(
        !f.work
            .store
            .visible_tasks(&f.developer, &board_query)
            .unwrap()
            .items
            .iter()
            .any(|item| item.task.id == child.id)
    );
    assert!(
        !f.work
            .store
            .developer_board_tasks(&f.developer, None, 100)
            .unwrap()
            .items
            .iter()
            .any(|item| item.task.id == child.id)
    );
    // Completed stage work remains in the task pool and is still readable.
    assert!(
        f.work
            .store
            .visible_tasks(&f.developer, &VisibleTaskQuery::default())
            .unwrap()
            .items
            .iter()
            .any(|item| item.task.id == child.id)
    );

    assert_eq!(
        f.work
            .store
            .task_stages(&f.developer, &child.id)
            .unwrap()
            .len(),
        1
    );
    // Reopening within the same stage must invalidate current completion, while retaining history.
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
        "UPDATE tasks SET state_id='ready',revision=revision+1 WHERE id=$1",
        &[&child.id],
    )
    .unwrap();
    assert!(
        f.work
            .store
            .visible_tasks(&f.developer, &board_query)
            .unwrap()
            .items
            .iter()
            .any(|item| item.task.id == child.id)
    );

    assert!(
        f.work
            .store
            .transition_pool_task(
                &f.developer,
                &parent.id,
                "finish",
                &request(parent.revision)
            )
            .is_err()
    );
    let child = f.work.store.pool_task(&f.developer, &child.id).unwrap();
    let child = f
        .work
        .store
        .transition_pool_task(&f.developer, &child.id, "finish", &request(child.revision))
        .unwrap();
    assert_eq!(
        f.work
            .store
            .task_stages(&f.developer, &child.id)
            .unwrap()
            .len(),
        2
    );
    let parent = f
        .work
        .store
        .transition_pool_task(
            &f.developer,
            &parent.id,
            "finish",
            &request(parent.revision),
        )
        .unwrap();
    assert_eq!(parent.lifecycle, TaskLifecycle::Active);
    assert_eq!(parent.state_id.as_deref(), Some("done"));
    // Parent Fin stays visible while final delivery is pending.
    assert!(
        f.work
            .store
            .visible_tasks(&f.developer, &board_query)
            .unwrap()
            .items
            .iter()
            .any(|item| item.task.id == parent.id)
    );

    assert_eq!(
        f.work
            .store
            .task_stages(&f.developer, &parent.id)
            .unwrap()
            .len(),
        1
    );
}
