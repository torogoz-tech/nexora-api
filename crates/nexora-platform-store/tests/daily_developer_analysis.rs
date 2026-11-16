#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::SecretGenerator;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
fn sql(f: &WorkItemFixture) -> postgres::Client {
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(f.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut c = postgres::Config::new()
        .host(&f.config.host)
        .port(f.config.postgres_port)
        .user(&f.config.postgres_user)
        .password(password.trim())
        .dbname(&f.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    c.batch_execute(&format!("SET search_path TO \"{}\"", f.namespace))
        .unwrap();
    c
}
fn member(f: &mut WorkItemFixture, login: &str, role: MemberRole) -> AuthContext {
    let password = zeroize::Zeroizing::new(SecretGenerator::token("password").unwrap());
    let actor = f
        .store
        .create_user(&f.ctx, login, &password, login)
        .unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &actor.id,
            &MemberRoleChange {
                role,
                all_projects: false,
                project_ids: vec![f.project.id.clone()],
                expected_revision: 0,
            },
        )
        .unwrap();
    let session = f.store.login(login, &password).unwrap();
    let verified = f
        .store
        .identity_gate(&session.access_token, "skip", "")
        .unwrap();
    f.store
        .authenticate(verified["access_token"].as_str().unwrap())
        .unwrap()
}
#[test]
#[ignore = "requires PostgreSQL"]
fn daily_analysis_authorization_dates_idempotency_and_fencing() {
    let mut f = WorkItemFixture::new();
    let dev = member(&mut f, "daily_dev", MemberRole::Developer);
    let lead = member(&mut f, "daily_lead", MemberRole::TechnicalLead);
    let mut c = sql(&f);
    assert!(f.store.daily_developer_analysis_directory(&dev).is_err());
    assert!(
        f.store.daily_developer_analysis_directory(&lead).unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == dev.actor_id())
    );
    assert!(
        f.store
            .daily_developer_analysis(&lead, dev.actor_id(), "2026-02-30")
            .is_err()
    );
    assert!(
        f.store
            .daily_developer_analysis(&lead, dev.actor_id(), "9999-01-01")
            .is_err()
    );
    // Set the member's origin before the closed day; organization timezone defines UTC bounds.
    c.execute(
        "UPDATE actors SET created_at=clock_timestamp()-interval '3 days' WHERE id=$1",
        &[&dev.actor_id()],
    )
    .unwrap();
    c.execute("UPDATE labor_calendar_settings SET timezone='America/El_Salvador',starts_on=(clock_timestamp() AT TIME ZONE 'America/El_Salvador')::date-10",&[]).unwrap();
    let date:String=c.query_one("SELECT ((clock_timestamp() AT TIME ZONE timezone)::date-1)::text FROM labor_calendar_settings WHERE singleton",&[]).unwrap().get(0);
    f.store.schedule_daily_developer_analyses().unwrap();
    f.store.schedule_daily_developer_analyses().unwrap();
    assert_eq!(c.query_one("SELECT count(*) FROM daily_developer_analyses WHERE actor_id=$1 AND date=$2::text::date",&[&dev.actor_id(),&date]).unwrap().get::<_,i64>(0),1);
    // Leave only this member queued and add a concrete knowledge contribution on that local day.
    c.execute(
        "DELETE FROM daily_developer_analyses WHERE actor_id<>$1",
        &[&dev.actor_id()],
    )
    .unwrap();
    c.execute("INSERT INTO organization_documents(id,kind,title,body,created_by) VALUES('daily_doc','document','Retry policy','Retries require idempotency',$1)",&[&dev.actor_id()]).unwrap();
    c.execute("INSERT INTO organization_document_versions(document_id,revision,title,body,created_by,created_at) VALUES('daily_doc',1,'Retry policy','Retries require idempotency',$1,($2::text::date+time '12:00') AT TIME ZONE 'America/El_Salvador')",&[&dev.actor_id(),&date]).unwrap();
    f.store
        .save_personal_work_block(
            &dev,
            None,
            &json!({"title":"Training","date":date,"start_time":"09:00","end_time":"11:00"}),
            false,
        )
        .unwrap();
    // Two independent database connections contend for the same member/day.
    let mut other = nexora_platform_store::PlatformStore::connect(&f.config, &f.namespace).unwrap();
    let barrier = std::sync::Barrier::new(2);
    let (first, second) = std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            barrier.wait();
            f.store.claim_daily_developer_analysis().unwrap()
        });
        let second = scope.spawn(|| {
            barrier.wait();
            other.claim_daily_developer_analysis().unwrap()
        });
        (first.join().unwrap(), second.join().unwrap())
    });
    assert_eq!(
        usize::from(first.is_some()) + usize::from(second.is_some()),
        1
    );
    let job = first.or(second).unwrap();
    assert_eq!(job["date"], date);
    assert_eq!(job["evidence"]["counts"]["knowledge"], 1);
    assert_eq!(job["evidence"]["counts"]["work_log"], 1);
    assert!(job["evidence"]["availability"]["expected_task_minutes"].is_number());
    assert!(f.store.claim_daily_developer_analysis().unwrap().is_none());
    let mut stale = job.clone();
    stale["lease_token"] = json!("wrong");
    assert!(
        !f.store
            .finish_daily_developer_analysis(&stale, Some(&json!({"summary":"wrong"})))
            .unwrap()
    );
    assert!(
        f.store
            .finish_daily_developer_analysis(
                &job,
                Some(&json!({"summary":"Documented retry policy."}))
            )
            .unwrap()
    );
    assert!(!f.store.finish_daily_developer_analysis(&job, None).unwrap());
    assert_eq!(
        f.store
            .daily_developer_analysis(&lead, dev.actor_id(), &date)
            .unwrap()["report"]["summary"],
        "Documented retry policy."
    );
    assert!(
        f.store
            .daily_developer_analysis(&dev, dev.actor_id(), &date)
            .is_err()
    );
    // Cached reads never enqueue or reopen a completed job.
    let before: i64 = c
        .query_one("SELECT count(*) FROM daily_developer_analyses", &[])
        .unwrap()
        .get(0);
    f.store
        .daily_developer_analysis(&lead, dev.actor_id(), &date)
        .unwrap();
    assert_eq!(
        c.query_one("SELECT count(*) FROM daily_developer_analyses", &[])
            .unwrap()
            .get::<_, i64>(0),
        before
    );
    let private_project = f
        .store
        .create_project(&f.ctx, "Historical private", None)
        .unwrap();
    c.execute("UPDATE daily_developer_analyses SET evidence=jsonb_set(evidence,'{project_ids}',$2) WHERE actor_id=$1",&[&dev.actor_id(),&json!([private_project.id])]).unwrap();
    assert!(
        f.store
            .daily_developer_analysis(&lead, dev.actor_id(), &date)
            .is_err()
    );
    assert!(
        f.store
            .daily_developer_analysis(&f.ctx, dev.actor_id(), &date)
            .is_ok()
    );
    c.execute("INSERT INTO daily_developer_analyses(actor_id,date,timezone) VALUES($1,$2::text::date-1,'America/El_Salvador')",&[&dev.actor_id(),&date]).unwrap();
    assert!(f.store.claim_daily_developer_analysis().unwrap().is_none());
    assert_eq!(c.query_one("SELECT status FROM daily_developer_analyses WHERE actor_id=$1 AND date=$2::text::date-1",&[&dev.actor_id(),&date]).unwrap().get::<_,String>(0),"no_evidence");
    // Revocation takes effect using the same previously authenticated context.
    c.execute(
        "UPDATE actor_roles SET role='developer' WHERE actor_id=$1",
        &[&lead.actor_id()],
    )
    .unwrap();
    assert!(
        f.store
            .daily_developer_analysis(&lead, dev.actor_id(), &date)
            .is_err()
    );
}
