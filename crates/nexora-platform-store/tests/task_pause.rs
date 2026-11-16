#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
#[path = "support/task_test_sql.rs"]
mod test_sql;
use nexora_platform::*;
use serde_json::json;
use task_pool_fixture::TaskPoolFixture;

#[test]
#[ignore = "requires PostgreSQL"]
fn pause_preserves_workflow_excludes_time_and_archive_requires_leadership() {
    let mut f = TaskPoolFixture::new(false);
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    let mut task = f
        .work
        .store
        .create_pool_task(
            &f.work.ctx,
            &serde_json::from_value(
                json!({"title":"Pause me","type_code":"task","origin":"test","fields":{}}),
            )
            .unwrap(),
        )
        .unwrap();
    let archive = TaskChange {
        expected_revision: task.revision,
        archived: Some(true),
        reason: "Archive".into(),
        ..Default::default()
    };
    assert!(matches!(
        f.work
            .store
            .change_pool_task(&f.developer, &task.id, &archive),
        Err(PlatformError::Forbidden)
    ));
    task = f
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
    let profile = f
        .work
        .store
        .member_profile(&f.developer, f.developer.actor_id(), Some(2026))
        .unwrap();
    assert_eq!(profile["workflow_tasks"][0]["id"], task.id);
    assert_eq!(
        profile["workflow_tasks"][0]["project_id"],
        serde_json::Value::Null
    );
    let state = task.state_id.clone();
    let workflow = task.workflow_version_id.clone();
    assert!(
        f.work
            .store
            .set_task_paused(&f.other, &task.id, task.revision, true)
            .is_err()
    );
    task = f
        .work
        .store
        .set_task_paused(&f.developer, &task.id, task.revision, true)
        .unwrap();
    assert_eq!(task.lifecycle, TaskLifecycle::Paused);
    assert_eq!(task.state_id, state);
    assert_eq!(task.workflow_version_id, workflow);
    assert!(
        f.work
            .store
            .developer_board_tasks(&f.developer, None, 100)
            .unwrap()
            .items
            .is_empty()
    );
    assert!(
        f.work
            .store
            .transition_pool_task(
                &f.developer,
                &task.id,
                "finish",
                &TaskTransitionRequest {
                    expected_revision: task.revision,
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert!(
        f.work
            .store
            .change_pool_task(
                &f.other,
                &task.id,
                &TaskChange {
                    expected_revision: task.revision,
                    ..archive.clone()
                }
            )
            .is_err()
    );
    assert!(
        f.work
            .store
            .set_task_paused(&f.developer, &task.id, task.revision - 1, false)
            .is_err()
    );
    task = f
        .work
        .store
        .set_task_paused(&f.developer, &task.id, task.revision, false)
        .unwrap();
    assert_eq!(task.state_id, state);
    assert_eq!(task.workflow_version_id, workflow);
    assert_eq!(
        f.work
            .store
            .developer_board_tasks(&f.developer, None, 100)
            .unwrap()
            .items
            .len(),
        1
    );
    task = f
        .work
        .store
        .set_task_paused(&f.developer, &task.id, task.revision, true)
        .unwrap();
    sql.execute("UPDATE task_history SET created_at=now()-CASE operation WHEN 'task.created' THEN interval '4 hours' WHEN 'task.taken' THEN interval '3 hours' WHEN 'task.resume' THEN interval '1 hour' WHEN 'task.pause' THEN CASE WHEN sequence=(SELECT min(sequence) FROM task_history WHERE task_id=$1 AND operation='task.pause') THEN interval '2 hours' ELSE interval '30 minutes' END ELSE interval '0 hours' END WHERE task_id=$1",&[&task.id]).unwrap();
    let page = f
        .work
        .store
        .pool_task_presentation(&f.developer, &task.id)
        .unwrap();
    assert_eq!(page["timing"]["active_seconds"], 5400);
    let profile = f
        .work
        .store
        .member_profile(&f.developer, f.developer.actor_id(), Some(2026))
        .unwrap();
    assert_eq!(profile["workflow_tasks"].as_array().unwrap().len(), 0);
    task = f
        .work
        .store
        .change_pool_task(
            &f.manager,
            &task.id,
            &TaskChange {
                expected_revision: task.revision,
                clear_assignee: true,
                reason: "Return to pool".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(task.lifecycle, TaskLifecycle::Pending);
    assert!(
        f.work
            .store
            .change_pool_task(
                &f.manager,
                &task.id,
                &TaskChange {
                    expected_revision: task.revision,
                    ..archive
                }
            )
            .unwrap()
            .archived
    );
}

#[test]
#[ignore = "requires PostgreSQL"]
fn weekly_work_deduplicates_tasks_splits_local_midnight_and_approved_overtime() {
    let mut f = TaskPoolFixture::new(false);
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    sql.batch_execute("UPDATE labor_calendar_settings SET timezone='America/El_Salvador',work_start='08:00',work_end='17:00',break_minutes=60,daily_minutes=480").unwrap();
    for _ in 0..2 {
        let task = f
            .work
            .store
            .create_pool_task(
                &f.work.ctx,
                &serde_json::from_value(
                    json!({"title":"Week task","type_code":"task","origin":"test","fields":{}}),
                )
                .unwrap(),
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
        let task = f
            .work
            .store
            .set_task_paused(&f.developer, &task.id, task.revision, true)
            .unwrap();
        sql.execute("UPDATE task_history SET created_at=CASE operation WHEN 'task.created' THEN '2026-11-09T13:00:00Z'::timestamptz WHEN 'task.taken' THEN '2026-11-09T22:00:00Z'::timestamptz ELSE '2026-11-10T07:00:00Z'::timestamptz END WHERE task_id=$1",&[&task.id]).unwrap();
    }
    sql.execute("INSERT INTO labor_overtime_requests(id,actor_id,date,timezone,start_time,end_time,next_day,minutes,reason,status,decided_by) VALUES('overtime-test',$1,'2026-11-09','America/El_Salvador','17:00','00:00',true,420,'test','approved',$2)",&[&f.developer.actor_id(),&f.manager.actor_id()]).unwrap();
    let week = f
        .work
        .store
        .member_work_week(&f.developer, f.developer.actor_id(), Some("2026-11-09"))
        .unwrap();
    assert_eq!(week["days"][0]["active_seconds"], 28800, "{week}");
    assert_eq!(week["days"][0]["regular_seconds"], 3600);
    assert_eq!(week["days"][0]["overtime_seconds"], 25200);
    assert_eq!(week["days"][1]["active_seconds"], 3600);
    assert_eq!(week["days"][1]["unapproved_seconds"], 3600);
    assert!(
        f.work
            .store
            .member_work_week(&f.developer, f.manager.actor_id(), None)
            .is_err()
    );
    assert!(
        f.work
            .store
            .member_work_week(&f.developer, f.developer.actor_id(), Some("invalid"))
            .is_err()
    );
}
