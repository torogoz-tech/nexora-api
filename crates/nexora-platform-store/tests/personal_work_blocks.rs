#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
#[path = "support/task_test_sql.rs"]
mod test_sql;
use nexora_platform::*;
use serde_json::json;
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL"]
fn calendar_blocks_share_ledger_are_private_revisioned_and_deduct_only_independent_time() {
    let mut f = TaskPoolFixture::new(false);
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    sql.batch_execute("UPDATE labor_calendar_settings SET work_start='08:00',work_end='17:00',break_minutes=60,daily_minutes=480,workdays=ARRAY[1,2,3,4,5]").unwrap();
    let mut input =
        json!({"title":"Training","date":"2026-11-09","start_time":"09:00","end_time":"11:00"});
    let block = f
        .work
        .store
        .request(
            &f.developer,
            "calendar.work_block.create",
            "block-retry",
            &input,
            |store| store.save_personal_work_block(&f.developer, None, &input, false),
        )
        .unwrap();
    let repeated = f
        .work
        .store
        .request(
            &f.developer,
            "calendar.work_block.create",
            "block-retry",
            &input,
            |store| store.save_personal_work_block(&f.developer, None, &input, false),
        )
        .unwrap();
    assert_eq!(block["id"], repeated["id"]);
    let id = block["id"].as_str().unwrap();
    assert!(
        f.work
            .store
            .personal_work_blocks(&f.other, "2026-11-09", "2026-11-09")
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        f.work
            .store
            .save_personal_work_block(&f.other, Some(id), &input, false),
        Err(PlatformError::NotFound)
    ));
    assert!(matches!(
        f.work
            .store
            .save_personal_work_block(&f.developer, None, &input, false),
        Err(PlatformError::Conflict(_))
    ));
    let week = f
        .work
        .store
        .member_work_week(&f.developer, f.developer.actor_id(), Some("2026-11-09"))
        .unwrap();
    assert_eq!(week["days"][0]["active_seconds"], 7200);
    assert_eq!(
        week["days"][0]["availability"]["expected_task_minutes"],
        360
    );
    let task = f.create(Some(f.developer.actor_id().into()), None);
    input["task_id"] = json!(task.id);
    input["expected_revision"] = json!(1);
    f.work
        .store
        .save_personal_work_block(&f.developer, Some(id), &input, false)
        .unwrap();
    assert!(
        f.work
            .store
            .save_personal_work_block(&f.developer, Some(id), &input, false)
            .is_err()
    );
    assert_eq!(
        f.work
            .store
            .task_work_logs(&f.developer, &task.id)
            .unwrap()
            .len(),
        1
    );
    let week = f
        .work
        .store
        .member_work_week(&f.developer, f.developer.actor_id(), Some("2026-11-09"))
        .unwrap();
    assert_eq!(week["days"][0]["active_seconds"], 7200);
    assert_eq!(
        week["days"][0]["availability"]["expected_task_minutes"],
        480
    );
    let manual = TaskWorkLogInput {
        date: "2026-11-09".into(),
        minutes: 60,
        note: None,
        expected_revision: Some(2),
        overtime_request_id: None,
    };
    assert!(
        f.work
            .store
            .save_task_work_log(&f.developer, &task.id, Some(id), &manual, false)
            .is_err()
    );
    input["expected_revision"] = json!(2);
    f.work
        .store
        .save_personal_work_block(&f.developer, Some(id), &input, true)
        .unwrap();
    assert!(
        f.work
            .store
            .task_work_logs(&f.developer, &task.id)
            .unwrap()
            .is_empty()
    );
}
#[test]
#[ignore = "requires PostgreSQL"]
fn availability_unions_events_and_blocks_and_excludes_approved_absences() {
    let mut f = TaskPoolFixture::new(false);
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    sql.batch_execute("UPDATE labor_calendar_settings SET work_start='08:00',work_end='17:00',break_minutes=60,daily_minutes=480,workdays=ARRAY[1,2,3,4,5]").unwrap();
    f.work.store.save_personal_work_block(&f.developer,None,&json!({"title":"Training","date":"2026-11-09","start_time":"09:00","end_time":"11:00"}),false).unwrap();
    sql.execute("INSERT INTO team_calendar_activities(id,created_by,title,description,location,start_date,end_date,all_day,start_time,end_time) VALUES('availability-event',$1,'Meeting','','','2026-11-09','2026-11-09',false,'10:00','12:00')",&[&f.manager.actor_id()]).unwrap();
    let get = |sql: &mut postgres::Client| -> serde_json::Value {
        sql.query_one(
            "SELECT member_daily_availability($1,'2026-11-09')",
            &[&f.developer.actor_id()],
        )
        .unwrap()
        .get(0)
    };
    assert_eq!(get(&mut sql)["expected_task_minutes"], 300);
    sql.execute("INSERT INTO calendar_activity_attendance(activity_id,actor_id,activity_revision,response) VALUES('availability-event',$1,0,'declined')",&[&f.developer.actor_id()]).unwrap();
    assert_eq!(get(&mut sql)["expected_task_minutes"], 360);
    sql.execute("INSERT INTO labor_leave_requests(id,actor_id,kind,start_date,end_date,reason,status) VALUES('availability-leave',$1,'vacation','2026-11-09','2026-11-09','','approved')",&[&f.developer.actor_id()]).unwrap();
    assert_eq!(get(&mut sql)["expected_task_minutes"], 0);
    assert!(
        sql.query_one(
            "SELECT developer_rating_excluded($1,'2026-11-09')",
            &[&f.developer.actor_id()]
        )
        .unwrap()
        .get::<_, bool>(0)
    );
}

#[test]
#[ignore = "requires PostgreSQL"]
fn work_blocks_and_task_logs_share_the_same_approved_overtime_budget() {
    let mut f = TaskPoolFixture::new(false);
    let task = f.create(Some(f.developer.actor_id().into()), None);
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    sql.execute("INSERT INTO labor_overtime_requests(id,actor_id,date,start_time,end_time,next_day,minutes,reason,timezone,status) VALUES('block-ot',$1,'2026-11-09','18:00','19:00',false,60,'Test','UTC','approved')",&[&f.developer.actor_id()]).unwrap();
    f.work
        .store
        .save_task_work_log(
            &f.developer,
            &task.id,
            None,
            &TaskWorkLogInput {
                date: "2026-11-09".into(),
                minutes: 45,
                note: None,
                expected_revision: None,
                overtime_request_id: Some("block-ot".into()),
            },
            false,
        )
        .unwrap();
    let mut input = json!({"title":"Training","date":"2026-11-09","start_time":"18:00","end_time":"18:30","overtime_request_id":"block-ot"});
    assert!(matches!(
        f.work
            .store
            .save_personal_work_block(&f.developer, None, &input, false),
        Err(PlatformError::Validation(_))
    ));
    input["end_time"] = json!("18:15");
    f.work
        .store
        .save_personal_work_block(&f.developer, None, &input, false)
        .unwrap();
    let week = f
        .work
        .store
        .member_work_week(&f.developer, f.developer.actor_id(), Some("2026-11-09"))
        .unwrap();
    assert_eq!(week["days"][0]["overtime_seconds"], 3600);
    input["task_id"] = json!(42);
    assert!(matches!(
        f.work
            .store
            .save_personal_work_block(&f.developer, None, &input, false),
        Err(PlatformError::Validation(_))
    ));
}
#[test]
#[ignore = "requires PostgreSQL"]
fn availability_clips_calendar_time_to_workdays_and_excludes_holidays() {
    let mut f = TaskPoolFixture::new(false);
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    sql.batch_execute("UPDATE labor_calendar_settings SET work_start='08:00',work_end='17:00',break_minutes=60,daily_minutes=480,workdays=ARRAY[1,2,3,4,5]").unwrap();
    sql.execute("INSERT INTO team_calendar_activities(id,created_by,title,description,location,start_date,end_date,all_day,start_time,end_time) VALUES('off-hours',$1,'Evening','','','2026-11-09','2026-11-09',false,'18:00','20:00')",&[&f.manager.actor_id()]).unwrap();
    let availability: serde_json::Value = sql
        .query_one(
            "SELECT member_daily_availability($1,'2026-11-09')",
            &[&f.developer.actor_id()],
        )
        .unwrap()
        .get(0);
    assert_eq!(availability["expected_task_minutes"], 480);
    let weekend: serde_json::Value = sql
        .query_one(
            "SELECT member_daily_availability($1,'2026-11-08')",
            &[&f.developer.actor_id()],
        )
        .unwrap()
        .get(0);
    assert_eq!(weekend["expected_task_minutes"], 0);
    sql.execute("INSERT INTO labor_holidays(id,name,date,status,proposed_by,reason) VALUES('availability-holiday','Holiday','2026-11-09','approved',$1,'')",&[&f.manager.actor_id()]).unwrap();
    let holiday: serde_json::Value = sql
        .query_one(
            "SELECT member_daily_availability($1,'2026-11-09')",
            &[&f.developer.actor_id()],
        )
        .unwrap()
        .get(0);
    assert_eq!(holiday["expected_task_minutes"], 0);
}
