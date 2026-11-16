#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use nexora_platform_store::{InAppNotificationChange, InAppNotificationQuery};
use serde_json::json;
use task_pool_fixture::TaskPoolFixture;

fn page(folder: &str) -> InAppNotificationQuery {
    InAppNotificationQuery {
        folder: folder.into(),
        limit: 100,
        offset: 0,
    }
}

#[test]
#[ignore = "requires PostgreSQL; durable personal inbox, transactional events, resource authorization and state isolation"]
fn assignment_comment_calendar_inbox_is_durable_personal_and_authorized() {
    let mut f = TaskPoolFixture::new(false);
    let project = f.work.project.id.clone();
    let developer = f.developer.actor_id().to_owned();
    let task = f.create(Some(developer.clone()), Some(project.clone()));
    let inbox = f
        .work
        .store
        .in_app_notifications(&f.developer, &page("inbox"))
        .unwrap();
    let assignment = inbox["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["kind"] == "assignment" && n["resource_id"] == task.id)
        .unwrap();
    let id = assignment["id"].as_str().unwrap().to_owned();
    assert!(assignment["body"].as_str().unwrap().contains(&task.title));
    assert_eq!(assignment["url"], format!("/tasks/{}", task.id));
    assert!(
        f.work
            .store
            .claim_email_notification(true)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        f.work.store.change_in_app_notification(
            &f.other,
            &id,
            &InAppNotificationChange {
                read: Some(true),
                archived: None
            }
        ),
        Err(PlatformError::NotFound)
    ));
    let first_read = f
        .work
        .store
        .change_in_app_notification(
            &f.developer,
            &id,
            &InAppNotificationChange {
                read: Some(true),
                archived: None,
            },
        )
        .unwrap();
    let repeated_read = f
        .work
        .store
        .change_in_app_notification(
            &f.developer,
            &id,
            &InAppNotificationChange {
                read: Some(true),
                archived: None,
            },
        )
        .unwrap();
    assert_eq!(first_read["read_at"], repeated_read["read_at"]);

    assert!(
        !f.work
            .store
            .in_app_notifications(&f.developer, &page("unread"))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == id)
    );
    f.work
        .store
        .change_in_app_notification(
            &f.developer,
            &id,
            &InAppNotificationChange {
                read: Some(false),
                archived: Some(true),
            },
        )
        .unwrap();
    assert!(
        f.work
            .store
            .in_app_notifications(&f.developer, &page("archived"))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == id)
    );
    f.work
        .store
        .change_in_app_notification(
            &f.developer,
            &id,
            &InAppNotificationChange {
                read: Some(false),
                archived: Some(false),
            },
        )
        .unwrap();
    f.work
        .store
        .add_task_comment(
            &f.manager,
            &project,
            &task.id,
            &TaskCommentCreate {
                body: "Private comment content is never copied into the inbox".into(),
            },
        )
        .unwrap();
    let inbox = f
        .work
        .store
        .in_app_notifications(&f.developer, &page("unread"))
        .unwrap();
    assert!(
        inbox["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["kind"] == "task_comment")
    );
    assert!(!inbox.to_string().contains("Private comment content"));
    let event=f.work.store.save_calendar_activity(&f.manager,None,&json!({"title":"Inbox team event","start_date":"2026-11-10","end_date":"2026-11-10","all_day":true,"attendance_required":true}),false).unwrap();
    let calendar = f
        .work
        .store
        .in_app_notifications(&f.developer, &page("unread"))
        .unwrap();
    assert!(
        calendar["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["kind"] == "calendar_created"
                && n["body"].as_str().unwrap().contains("Inbox team event"))
    );
    f.work
        .store
        .respond_calendar_activity(
            &f.developer,
            event["id"].as_str().unwrap(),
            &json!({"expected_revision":0,"response":"attending"}),
        )
        .unwrap();
    assert!(
        f.work
            .store
            .in_app_notifications(&f.manager, &page("unread"))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["kind"] == "calendar_attendance")
    );
    let other_project = f
        .work
        .store
        .create_project(&f.work.ctx, "Other scope", None)
        .unwrap();
    let assignment = f.work.store.member_role(&f.work.ctx, &developer).unwrap();
    f.work
        .store
        .change_member_role(
            &f.work.ctx,
            &developer,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![other_project.id],
                expected_revision: assignment.revision,
            },
        )
        .unwrap();
    let after = f
        .work
        .store
        .in_app_notifications(&f.developer, &page("inbox"))
        .unwrap();
    assert!(
        !after["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["resource_id"] == task.id)
    );
    f.work
        .store
        .read_all_in_app_notifications(&f.developer)
        .unwrap();
    assert_eq!(
        f.work
            .store
            .in_app_notifications(&f.developer, &page("unread"))
            .unwrap()["unread_count"],
        0
    );
    assert!(matches!(
        f.work
            .store
            .in_app_notifications(&f.developer, &page("arbitrary")),
        Err(PlatformError::Validation(_))
    ));
}

#[test]
#[ignore = "requires PostgreSQL; event categories, rollback, current approver filtering and localized context"]
fn labor_and_repository_results_are_atomic_and_contextual() {
    let mut f = TaskPoolFixture::new(false);
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
    let developer = f.developer.actor_id().to_owned();
    let manager = f.manager.actor_id().to_owned();
    {
        let mut tx = sql.transaction().unwrap();
        tx.execute("INSERT INTO labor_leave_requests(id,actor_id,kind,start_date,end_date,reason) VALUES('rolled_back',$1,'vacation','2026-11-10','2026-11-10','Private reason')",&[&developer]).unwrap();
        tx.rollback().unwrap();
    }
    assert_eq!(
        sql.query_one(
            "SELECT count(*) FROM in_app_notifications WHERE resource_id='rolled_back'",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        0
    );
    sql.execute("INSERT INTO labor_leave_requests(id,actor_id,kind,start_date,end_date,reason) VALUES('inbox_leave',$1,'vacation','2026-11-10','2026-11-10','Private reason')",&[&developer]).unwrap();
    sql.execute("INSERT INTO labor_overtime_requests(id,actor_id,date,timezone,start_time,end_time,minutes,reason) VALUES('inbox_overtime',$1,'2026-11-10','UTC','18:00','19:00',60,'Private reason')",&[&developer]).unwrap();
    let requests = f
        .work
        .store
        .in_app_notifications(&f.manager, &page("unread"))
        .unwrap();
    assert!(
        requests["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["kind"] == "leave_request")
    );
    assert!(
        requests["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["kind"] == "overtime_request")
    );
    assert!(!requests.to_string().contains("Private reason"));
    sql.execute("UPDATE labor_leave_requests SET status='approved',decided_by=$1,decision_reason='Private decision',decided_at=clock_timestamp() WHERE id='inbox_leave'",&[&manager]).unwrap();
    sql.execute("UPDATE labor_overtime_requests SET status='rejected',decided_by=$1,decision_reason='Private decision',decided_at=clock_timestamp() WHERE id='inbox_overtime'",&[&manager]).unwrap();
    let results = f
        .work
        .store
        .in_app_notifications(&f.developer, &page("unread"))
        .unwrap();
    assert!(
        results["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["kind"] == "leave_result" && n["decision"] == "approved")
    );
    assert!(
        results["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["kind"] == "overtime_result" && n["decision"] == "rejected")
    );
    assert!(!results.to_string().contains("Private decision"));
    sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES('inbox_connection','gitlab','gitlab.com',$1,'inbox-account','inbox-fixture','active',1,'{}',clock_timestamp()+interval '1 hour')",&[&f.work.ctx.actor_id()]).unwrap();
    sql.execute("INSERT INTO project_repositories(id,project_id,provider,host,requested_full_name,remote_id,remote_full_name,connection_id,authorization_status) VALUES('repo_inbox_fixture',$1,'gitlab','gitlab.com','example/inbox','inbox-remote','example/inbox','inbox_connection','authorized')",&[&f.work.project.id]).unwrap();
    sql.execute("INSERT INTO pull_request_reviews(id,project_id,repository_id,pr_number,requested_by,request_id,expected_head_sha,source,input,status) VALUES('review_inbox',$1,'repo_inbox_fixture',1,$2,'inbox-test-request','abc','{}','{}','running')",&[&f.work.project.id,&manager]).unwrap();
    sql.execute("UPDATE pull_request_reviews SET status='completed',finished_at=clock_timestamp() WHERE id='review_inbox'",&[]).unwrap();
    sql.execute(
        "UPDATE pull_request_reviews SET status='completed' WHERE id='review_inbox'",
        &[],
    )
    .unwrap();
    let reviews = f
        .work
        .store
        .in_app_notifications(&f.manager, &page("unread"))
        .unwrap();
    assert_eq!(
        reviews["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["kind"] == "review_completed")
            .count(),
        1
    );
    assert!(
        reviews["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["kind"] == "review_completed"
                && n["body"].as_str().unwrap().contains("example/inbox"))
    );
    let unread_before = f
        .work
        .store
        .in_app_notifications(&f.manager, &page("unread"))
        .unwrap()["unread_count"]
        .as_i64()
        .unwrap();
    sql.execute(
        "UPDATE git_connections SET status='revoked' WHERE id='inbox_connection'",
        &[],
    )
    .unwrap();
    let revoked = f
        .work
        .store
        .in_app_notifications(&f.manager, &page("unread"))
        .unwrap();
    assert!(
        !revoked["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["kind"] == "review_completed")
    );
    assert_eq!(revoked["unread_count"].as_i64().unwrap(), unread_before - 1);
    sql.execute(
        "UPDATE git_connections SET status='active' WHERE id='inbox_connection'",
        &[],
    )
    .unwrap();
    let developer_scope = format!("actor:{}", developer);
    let manager_scope = format!("actor:{}", manager);
    let developer_before:i64 = sql.query_one("SELECT revision FROM live_update_revisions WHERE scope_id=$1 AND topic='notifications'",&[&developer_scope]).unwrap().get(0);
    let manager_before:i64 = sql.query_one("SELECT revision FROM live_update_revisions WHERE scope_id=$1 AND topic='notifications'",&[&manager_scope]).unwrap().get(0);
    f.work
        .store
        .read_all_in_app_notifications(&f.manager)
        .unwrap();
    let developer_after:i64 = sql.query_one("SELECT revision FROM live_update_revisions WHERE scope_id=$1 AND topic='notifications'",&[&developer_scope]).unwrap().get(0);
    let manager_after:i64 = sql.query_one("SELECT revision FROM live_update_revisions WHERE scope_id=$1 AND topic='notifications'",&[&manager_scope]).unwrap().get(0);
    assert_eq!(developer_before, developer_after);
    assert!(manager_after > manager_before);
    sql.execute(
        "UPDATE project_repositories SET archived=true WHERE id='repo_inbox_fixture'",
        &[],
    )
    .unwrap();
    assert!(
        !f.work
            .store
            .in_app_notifications(&f.manager, &page("inbox"))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["kind"] == "review_completed")
    );
}
