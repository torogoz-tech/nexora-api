#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::{PlatformStore, SecretGenerator};
use serde_json::json;
use work_item_fixture::WorkItemFixture;
fn member(
    f: &mut WorkItemFixture,
    name: &str,
    role: MemberRole,
    projects: Vec<String>,
) -> AuthContext {
    let password = zeroize::Zeroizing::new(SecretGenerator::token("working_hours_test").unwrap());
    let a = f.store.create_user(&f.ctx, name, &password, name).unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &a.id,
            &MemberRoleChange {
                role,
                all_projects: false,
                project_ids: projects,
                expected_revision: 0,
            },
        )
        .unwrap();
    let s = f.store.login(name, &password).unwrap();
    let verified = f.store.identity_gate(&s.access_token, "skip", "").unwrap();
    f.store
        .authenticate(verified["access_token"].as_str().unwrap())
        .unwrap()
}
#[test]
#[ignore = "requires PostgreSQL; settings isolation, overtime scopes, overnight intervals and concurrent decisions"]
fn work_hours_timezone_and_overtime_are_authorized_atomic_and_independent() {
    let mut f = WorkItemFixture::new();
    let project = f.project.id.clone();
    let dev = member(
        &mut f,
        "developer",
        MemberRole::Developer,
        vec![project.clone()],
    );
    let manager = member(
        &mut f,
        "manager",
        MemberRole::Manager,
        vec![project.clone()],
    );
    let other = member(&mut f, "other", MemberRole::Developer, vec![project]);
    f.store
        .assign_labor_approver(
            &f.ctx,
            dev.actor_id(),
            &json!({"approver_id":manager.actor_id(),"expected_revision":0}),
        )
        .unwrap();
    let task = f
        .store
        .create_task(
            &f.ctx,
            &f.project.id,
            &TaskCreate {
                title: "Overtime delivery".into(),
                type_code: "task".into(),
                backlog_id: Some(f.backlog.id.clone()),
                assignee_id: Some(dev.actor_id().into()),
                origin: "Fixture".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let old = f.store.labor_calendar_settings(&f.ctx).unwrap();
    assert_eq!(old["work_start"], "09:00");
    assert_eq!(old["work_end"], "17:00");
    let zone = f
        .store
        .change_installation_timezone(
            &f.ctx,
            &json!({"timezone":"America/El_Salvador","expected_revision":old["revision"]}),
        )
        .unwrap();
    assert!(f.store.change_labor_calendar(&f.ctx,&json!({"work_start":"08:00","work_end":"17:00","break_minutes":60,"expected_revision":old["revision"]})).is_err());
    let hours=f.store.change_labor_calendar(&f.ctx,&json!({"work_start":"22:00","work_end":"06:00","break_minutes":30,"workdays":[1,2,3,4,5],"expected_revision":zone["revision"]})).unwrap();
    assert_eq!(hours["daily_minutes"], 450);
    assert_eq!(hours["timezone"], "America/El_Salvador");
    let zinput = json!({"timezone":"Europe/Madrid","expected_revision":hours["revision"]});
    let zoned = f
        .store
        .request(
            &f.ctx,
            "installation.timezone.change",
            "timezone-once",
            &zinput,
            |s| s.change_installation_timezone(&f.ctx, &zinput),
        )
        .unwrap();
    let replay = f
        .store
        .request(
            &f.ctx,
            "installation.timezone.change",
            "timezone-once",
            &zinput,
            |s| s.change_installation_timezone(&f.ctx, &zinput),
        )
        .unwrap();
    assert_eq!(zoned, replay);
    let after = f.store.labor_calendar_settings(&f.ctx).unwrap();
    assert_eq!(after["work_start"], "22:00");
    assert_eq!(after["work_end"], "06:00");
    assert_eq!(after["break_minutes"], 30);
    assert!(
        f.store
            .change_installation_timezone(
                &dev,
                &json!({"timezone":"UTC","expected_revision":after["revision"]})
            )
            .is_err()
    );
    assert!(
        f.store
            .change_installation_timezone(
                &f.ctx,
                &json!({"timezone":"Moon/Crater","expected_revision":after["revision"]})
            )
            .is_err()
    );
    for invalid in [
        json!({"work_start":"08:00","expected_revision":after["revision"]}),
        json!({"work_start":"08:00","work_end":"17:00","break_minutes":540,"expected_revision":after["revision"]}),
        json!({"work_start":"08:00","work_end":"17:00","break_minutes":60,"daily_minutes":1,"expected_revision":after["revision"]}),
    ] {
        assert!(f.store.change_labor_calendar(&f.ctx, &invalid).is_err())
    }
    let legacy = f
        .store
        .change_labor_calendar(
            &f.ctx,
            &json!({"daily_minutes":480,"expected_revision":after["revision"]}),
        )
        .unwrap();
    assert_eq!(legacy["work_end"], "06:30");
    assert_eq!(legacy["daily_minutes"], 480);
    assert_eq!(legacy["timezone"], "Europe/Madrid");
    let input = json!({"date":"2026-11-07","start_time":"23:00","end_time":"01:00","next_day":true,"reason":"Finish approved delivery","task_ids":[task.id]});
    assert!(f.store.create_overtime_request(&manager, &input).is_err());
    assert!(f.store.create_overtime_request(&f.ctx, &input).is_err());
    let req = f
        .store
        .request(
            &dev,
            "overtime.request.create",
            "overtime-once",
            &input,
            |s| s.create_overtime_request(&dev, &input),
        )
        .unwrap();
    let replay = f
        .store
        .request(
            &dev,
            "overtime.request.create",
            "overtime-once",
            &input,
            |s| s.create_overtime_request(&dev, &input),
        )
        .unwrap();
    assert_eq!(req["id"], replay["id"]);
    assert_eq!(req["status"], "pending");
    assert_eq!(req["minutes"], 120);
    assert_eq!(req["timezone"], "Europe/Madrid");
    assert!(f.store.create_overtime_request(&dev, &input).is_err());
    let mut overlap = input.clone();
    overlap["date"] = json!("2026-11-08");
    overlap["start_time"] = json!("00:30");
    overlap["end_time"] = json!("01:30");
    overlap["next_day"] = json!(false);
    assert!(f.store.create_overtime_request(&dev, &overlap).is_err());
    let id = req["id"].as_str().unwrap().to_owned();
    let decision = json!({"approved":true,"reason":"Validated delivery"});
    assert!(
        f.store
            .decide_overtime_request(&dev, &id, &decision)
            .is_err()
    );
    assert!(
        f.store
            .decide_overtime_request(&other, &id, &decision)
            .is_err()
    );
    assert_eq!(
        f.store.overtime(&other).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        f.store.overtime(&manager).unwrap()["total_approved_minutes"],
        0
    );
    let mut second = PlatformStore::connect(&f.config, &f.namespace).unwrap();
    let mut third = PlatformStore::connect(&f.config, &f.namespace).unwrap();
    let m1 = manager.clone();
    let m2 = manager.clone();
    let i1 = id.clone();
    let i2 = id.clone();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let b1 = barrier.clone();
    let t1 = std::thread::spawn(move || {
        b1.wait();
        second
            .decide_overtime_request(&m1, &i1, &json!({"approved":true,"reason":"Validated"}))
            .is_ok()
    });
    let t2 = std::thread::spawn(move || {
        barrier.wait();
        third
            .decide_overtime_request(&m2, &i2, &json!({"approved":false,"reason":"Rejected"}))
            .is_ok()
    });
    assert_ne!(t1.join().unwrap(), t2.join().unwrap());
    let listed = f.store.overtime(&manager).unwrap();
    let item = &listed["items"][0];
    assert!(item["decided_by_name"].is_string());
    assert!(item["decided_at"].is_string());
    assert_eq!(
        listed["total_approved_minutes"],
        if item["status"] == "approved" { 120 } else { 0 }
    );
    assert!(!item["tasks"].as_array().unwrap().is_empty());
    assert!(
        f.store
            .decide_overtime_request(&manager, &id, &decision)
            .is_err()
    );
    // The stored timezone fixes the absolute window across General timezone changes.
    let current = f.store.installation_timezone(&f.ctx).unwrap();
    f.store
        .change_installation_timezone(
            &f.ctx,
            &json!({"timezone":"UTC","expected_revision":current["revision"]}),
        )
        .unwrap();
    let mut absolute = input.clone();
    absolute["date"] = json!("2026-11-10");
    absolute["start_time"] = json!("20:00");
    absolute["end_time"] = json!("21:00");
    absolute["next_day"] = json!(false);
    let absolute_request = f.store.create_overtime_request(&dev, &absolute).unwrap();
    let current = f.store.installation_timezone(&f.ctx).unwrap();
    f.store
        .change_installation_timezone(
            &f.ctx,
            &json!({"timezone":"Europe/Madrid","expected_revision":current["revision"]}),
        )
        .unwrap();
    absolute["start_time"] = json!("21:00");
    absolute["end_time"] = json!("22:00");
    assert!(f.store.create_overtime_request(&dev, &absolute).is_err());
    f.store
        .decide_overtime_request(
            &manager,
            absolute_request["id"].as_str().unwrap(),
            &json!({"approved":false,"reason":"Fixture reset"}),
        )
        .unwrap();
    for (start, end, next) in [
        ("01:00", "01:00", false),
        ("01:00", "02:00", true),
        ("25:00", "26:00", false),
        ("12:00", "11:00", false),
    ] {
        let mut invalid = input.clone();
        invalid["start_time"] = json!(start);
        invalid["end_time"] = json!(end);
        invalid["next_day"] = json!(next);
        assert!(f.store.create_overtime_request(&dev, &invalid).is_err())
    }
    let mut dst = input.clone();
    dst["date"] = json!("2026-03-29");
    dst["start_time"] = json!("02:30");
    dst["end_time"] = json!("03:30");
    dst["next_day"] = json!(false);
    assert!(matches!(
        f.store.create_overtime_request(&dev, &dst),
        Err(PlatformError::Validation(_))
    ));
    let mut valid = input.clone();
    valid["date"] = json!("2026-11-12");
    valid["start_time"] = json!("09:00");
    valid["end_time"] = json!("09:00");
    let full = f.store.create_overtime_request(&dev, &valid).unwrap();
    assert_eq!(full["minutes"], 1440);
    let mut task_reason = valid.clone();
    task_reason["date"] = json!("2026-11-13");
    task_reason.as_object_mut().unwrap().remove("reason");
    let derived_reason = f.store.create_overtime_request(&dev, &task_reason).unwrap();
    assert_eq!(derived_reason["reason"], "Tareas vinculadas");
    let key = f
        .store
        .issue_key(
            &f.ctx,
            dev.actor_id(),
            "Overtime key",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let api = f.store.authenticate(&key.key).unwrap();
    assert!(f.store.overtime(&api).is_err());
    assert!(f.store.create_overtime_request(&api, &valid).is_err());
    let outside = f.store.create_project(&f.ctx, "Outside", None).unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            manager.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Manager,
                all_projects: false,
                project_ids: vec![outside.id],
                expected_revision: 1,
            },
        )
        .unwrap();
    assert_eq!(
        f.store.overtime(&manager).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(
        f.store
            .decide_overtime_request(&manager, full["id"].as_str().unwrap(), &decision)
            .is_ok()
    );
}

#[test]
#[ignore = "requires PostgreSQL"]
fn every_member_can_request_overtime_but_managers_cannot_approve_their_own() {
    let mut f = WorkItemFixture::new();
    let project = f.project.id.clone();
    let manager = member(
        &mut f,
        "requesting_manager",
        MemberRole::Manager,
        vec![project.clone()],
    );
    let reviewer = member(
        &mut f,
        "reviewing_manager",
        MemberRole::Manager,
        vec![project.clone()],
    );
    let lead = member(
        &mut f,
        "lead",
        MemberRole::TechnicalLead,
        vec![project.clone()],
    );
    let pmo = member(&mut f, "pmo", MemberRole::Pmo, vec![project]);
    let task = f
        .store
        .create_task(
            &f.ctx,
            &f.project.id,
            &TaskCreate {
                title: "Manager support".into(),
                type_code: "task".into(),
                backlog_id: Some(f.backlog.id.clone()),
                assignee_id: Some(manager.actor_id().into()),
                origin: "Fixture".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let input = json!({"date":"2026-11-10","start_time":"18:00","end_time":"19:00","next_day":false,"reason":"Planned support","task_ids":[task.id]});
    for actor in [&f.ctx.clone(), &manager, &lead, &pmo] {
        assert_eq!(f.store.overtime(actor).unwrap()["can_request"], true);
    }
    let request = f.store.create_overtime_request(&manager, &input).unwrap();
    let id = request["id"].as_str().unwrap();
    let decision = json!({"approved":true,"reason":"Planned support approved"});
    assert!(matches!(
        f.store.decide_overtime_request(&manager, id, &decision),
        Err(PlatformError::Forbidden)
    ));
    assert!(matches!(
        f.store.decide_overtime_request(&lead, id, &decision),
        Err(PlatformError::Forbidden)
    ));
    assert!(
        f.store
            .decide_overtime_request(&reviewer, id, &decision)
            .is_ok()
    );
}
