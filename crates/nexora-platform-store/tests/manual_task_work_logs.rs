#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
#[path = "support/task_test_sql.rs"]
mod test_sql;
use nexora_platform::*;
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL"]
fn manual_work_logs_are_owned_revisioned_audited_and_summed_without_state_time() {
    let mut f = TaskPoolFixture::new(false);
    let task = f.create(Some(f.developer.actor_id().into()), None);
    let mut input = TaskWorkLogInput {
        date: "2026-11-09".into(),
        minutes: 75,
        note: Some("Implementation".into()),
        expected_revision: None,
        overtime_request_id: None,
    };
    let log = f
        .work
        .store
        .save_task_work_log(&f.developer, &task.id, None, &input, false)
        .unwrap();
    assert_eq!(log.actor_id, f.developer.actor_id());
    input.minutes = 90;
    input.expected_revision = Some(1);
    assert!(matches!(
        f.work
            .store
            .save_task_work_log(&f.other, &task.id, Some(&log.id), &input, false),
        Err(PlatformError::NotFound)
    ));
    let changed = f
        .work
        .store
        .save_task_work_log(&f.developer, &task.id, Some(&log.id), &input, false)
        .unwrap();
    assert_eq!(changed.revision, 2);
    assert!(matches!(
        f.work
            .store
            .save_task_work_log(&f.developer, &task.id, Some(&log.id), &input, false),
        Err(PlatformError::Conflict(_))
    ));
    let week = f
        .work
        .store
        .member_work_week(&f.developer, f.developer.actor_id(), Some("2026-11-09"))
        .unwrap();
    assert_eq!(week["days"][0]["active_seconds"], 5400);
    assert_eq!(week["days"][0]["regular_seconds"], 5400);
    input.expected_revision = Some(2);
    f.work
        .store
        .save_task_work_log(&f.developer, &task.id, Some(&log.id), &input, true)
        .unwrap();
    assert!(
        f.work
            .store
            .task_work_logs(&f.developer, &task.id)
            .unwrap()
            .is_empty()
    );
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    let count:i64=sql.query_one("SELECT count(*) FROM task_history WHERE task_id=$1 AND operation LIKE 'task.work_log_%'",&[&task.id]).unwrap().get(0);
    assert_eq!(count, 3);
    let week = f
        .work
        .store
        .member_work_week(&f.developer, f.developer.actor_id(), Some("2026-11-09"))
        .unwrap();
    assert_eq!(week["days"][0]["active_seconds"], 0);
    input.minutes = 0;
    assert!(matches!(
        f.work
            .store
            .save_task_work_log(&f.developer, &task.id, None, &input, false),
        Err(PlatformError::Validation(_))
    ));
}
#[test]
#[ignore = "requires PostgreSQL"]
fn approved_overtime_is_explicit_owned_date_bound_and_cannot_be_overspent() {
    let mut f = TaskPoolFixture::new(false);
    let task = f.create(Some(f.developer.actor_id().into()), None);
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    sql.execute("INSERT INTO labor_overtime_requests(id,actor_id,date,start_time,end_time,next_day,minutes,reason,timezone,status) VALUES('log-overtime',$1,'2026-11-09','18:00','19:00',false,60,'Test','UTC','approved')",&[&f.developer.actor_id()]).unwrap();
    let mut input = TaskWorkLogInput {
        date: "2026-11-09".into(),
        minutes: 45,
        note: None,
        expected_revision: None,
        overtime_request_id: Some("log-overtime".into()),
    };
    f.work
        .store
        .save_task_work_log(&f.developer, &task.id, None, &input, false)
        .unwrap();
    assert!(matches!(
        f.work
            .store
            .save_task_work_log(&f.developer, &task.id, None, &input, false),
        Err(PlatformError::Validation(_))
    ));
    input.date = "2026-11-10".into();
    input.minutes = 10;
    assert!(matches!(
        f.work
            .store
            .save_task_work_log(&f.developer, &task.id, None, &input, false),
        Err(PlatformError::Validation(_))
    ));
    input.date = "2026-11-09".into();
    assert!(matches!(
        f.work
            .store
            .save_task_work_log(&f.other, &task.id, None, &input, false),
        Err(PlatformError::Validation(_))
    ));
    let week = f
        .work
        .store
        .member_work_week(&f.developer, f.developer.actor_id(), Some("2026-11-09"))
        .unwrap();
    assert_eq!(week["days"][0]["overtime_seconds"], 2700);
    assert_eq!(week["days"][0]["regular_seconds"], 0);
}
