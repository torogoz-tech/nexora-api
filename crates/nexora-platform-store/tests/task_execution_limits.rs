#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
#[path = "support/task_test_sql.rs"]
mod test_sql;
use nexora_platform::*;
use task_pool_fixture::TaskPoolFixture;

#[test]
#[ignore = "requires PostgreSQL"]
fn developers_and_leads_can_take_and_resume_multiple_tasks() {
    let mut f = TaskPoolFixture::new(false);
    for actor in [f.developer.clone(), f.other.clone()] {
        let first = f.create(None, None);
        let second = f.create(None, None);
        let first = f
            .work
            .store
            .take_task(
                &actor,
                &first.id,
                &TaskTake {
                    expected_revision: first.revision,
                    workflow_id: None,
                },
                None,
            )
            .unwrap();
        let second = f
            .work
            .store
            .take_task(
                &actor,
                &second.id,
                &TaskTake {
                    expected_revision: second.revision,
                    workflow_id: None,
                },
                None,
            )
            .unwrap();
        assert_eq!(first.lifecycle, TaskLifecycle::Active);
        assert_eq!(second.lifecycle, TaskLifecycle::Active);
        let paused = f
            .work
            .store
            .set_task_paused(&actor, &first.id, first.revision, true)
            .unwrap();
        assert_eq!(
            f.work
                .store
                .set_task_paused(&actor, &paused.id, paused.revision, false)
                .unwrap()
                .lifecycle,
            TaskLifecycle::Active
        );
    }
}

#[test]
#[ignore = "requires PostgreSQL"]
fn manager_archives_running_task_with_a_pause_event_in_the_same_transaction() {
    let mut f = TaskPoolFixture::new(false);
    let task = f.create(None, None);
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
    let paused = f
        .work
        .store
        .set_task_paused(&f.manager, &task.id, task.revision, true)
        .unwrap();
    assert_eq!(paused.lifecycle, TaskLifecycle::Paused);
    assert!(matches!(
        f.work
            .store
            .set_task_paused(&f.manager, &task.id, paused.revision, false),
        Err(PlatformError::Forbidden)
    ));
    let active = f
        .work
        .store
        .set_task_paused(&f.developer, &task.id, paused.revision, false)
        .unwrap();
    let archived = f
        .work
        .store
        .change_pool_task(
            &f.manager,
            &task.id,
            &TaskChange {
                expected_revision: active.revision,
                archived: Some(true),
                reason: "Archive running task".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(archived.archived);
    assert_eq!(archived.lifecycle, TaskLifecycle::Paused);
    assert_eq!(archived.revision, active.revision + 2);
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    let operations: Vec<String> = sql
        .query(
            "SELECT operation FROM task_history WHERE task_id=$1 ORDER BY sequence DESC LIMIT 2",
            &[&task.id],
        )
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(operations, vec!["task.changed", "task.pause"]);
    assert!(
        f.work
            .store
            .developer_board_tasks(&f.developer, None, 100)
            .unwrap()
            .items
            .is_empty()
    );
}

#[test]
#[ignore = "requires PostgreSQL"]
fn feature_completion_is_manager_only_and_preserves_linked_tasks() {
    let mut f = TaskPoolFixture::new(false);
    let task = f.create(None, Some(f.work.project.id.clone()));
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    sql.execute("INSERT INTO project_features(id,project_id,name,created_by) VALUES('completion-feature',$1,'Feature',$2)",&[&f.work.project.id,&f.manager.actor_id()]).unwrap();
    sql.execute("INSERT INTO task_feature_associations(task_id,feature_id,linked_by) VALUES($1,'completion-feature',$2)",&[&task.id,&f.manager.actor_id()]).unwrap();
    let input = FeatureComplete {
        expected_revision: 1,
    };
    for actor in [&f.developer, &f.other] {
        assert!(matches!(
            f.work
                .store
                .complete_feature(actor, &f.work.project.id, "completion-feature", &input),
            Err(PlatformError::Forbidden)
        ));
    }
    assert_eq!(
        f.work
            .store
            .feature_catalog(&f.manager, &f.work.project.id)
            .unwrap()["can_complete"],
        true
    );
    let result = f
        .work
        .store
        .complete_feature(&f.manager, &f.work.project.id, "completion-feature", &input)
        .unwrap();
    assert_eq!(result["status"], "completed");
    assert_eq!(result["revision"], 2);
    assert!(
        f.work
            .store
            .complete_feature(&f.manager, &f.work.project.id, "completion-feature", &input)
            .is_err()
    );
    let lifecycle: String = sql
        .query_one("SELECT lifecycle FROM tasks WHERE id=$1", &[&task.id])
        .unwrap()
        .get(0);
    assert_eq!(lifecycle, "pending");
}

#[test]
#[ignore = "requires PostgreSQL"]
fn concurrent_starts_for_the_same_developer_both_succeed() {
    let mut f = TaskPoolFixture::new(false);
    let tasks = [f.create(None, None), f.create(None, None)];
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = tasks
        .into_iter()
        .map(|task| {
            let config = f.work.config.clone();
            let namespace = f.work.namespace.clone();
            let ctx = f.developer.clone();
            let barrier = barrier.clone();

            std::thread::spawn(move || {
                let mut store =
                    nexora_platform_store::PlatformStore::connect(&config, &namespace).unwrap();
                barrier.wait();
                store.take_task(
                    &ctx,
                    &task.id,
                    &TaskTake {
                        expected_revision: task.revision,
                        workflow_id: None,
                    },
                    None,
                )
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        2,
        "{results:?}"
    );
}
