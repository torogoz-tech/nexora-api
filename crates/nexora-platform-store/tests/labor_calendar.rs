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
    let password = zeroize::Zeroizing::new(SecretGenerator::token("calendar_test").unwrap());
    let actor = f.store.create_user(&f.ctx, name, &password, name).unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &actor.id,
            &MemberRoleChange {
                role,
                all_projects: projects.is_empty(),
                project_ids: projects,
                expected_revision: 0,
            },
        )
        .unwrap();
    let token = f.store.login(name, &password).unwrap();
    f.store.authenticate(&token.access_token).unwrap()
}
#[test]
#[ignore = "requires PostgreSQL; authorization, calendar validation and concurrent decisions"]
fn labor_calendar_scopes_decisions_validation_and_races() {
    let mut f = WorkItemFixture::new();
    let project = f.project.id.clone();
    let dev = member(
        &mut f,
        "developer",
        MemberRole::Developer,
        vec![project.clone()],
    );
    let other = member(
        &mut f,
        "other",
        MemberRole::Developer,
        vec![project.clone()],
    );
    let manager = member(&mut f, "manager", MemberRole::Manager, vec![project]);
    let outside = f.store.create_project(&f.ctx, "Outside", None).unwrap();
    let outsider = member(
        &mut f,
        "outsider",
        MemberRole::Manager,
        vec![outside.id.clone()],
    );
    assert!(
        f.store
            .assign_labor_approver(
                &f.ctx,
                dev.actor_id(),
                &json!({"approver_id":outsider.actor_id(),"expected_revision":0})
            )
            .is_err()
    );
    f.store
        .assign_labor_approver(
            &f.ctx,
            dev.actor_id(),
            &json!({"approver_id":manager.actor_id(),"expected_revision":0}),
        )
        .unwrap();
    assert!(
        f.store
            .assign_labor_approver(
                &f.ctx,
                dev.actor_id(),
                &json!({"approver_id":manager.actor_id(),"expected_revision":0})
            )
            .is_err()
    );
    let input = json!({"kind":"vacation","start_date":"2026-01-01","end_date":"2026-01-02","minutes_per_day":null,"reason":"Family"});
    assert_eq!(
        f.store.labor_calendar(&manager).unwrap()["can_request"],
        true
    );
    let request = f
        .store
        .request(&dev, "labor.request.create", "calendar-once", &input, |s| {
            s.create_labor_leave_request(&dev, &input)
        })
        .unwrap();
    let replay = f
        .store
        .request(&dev, "labor.request.create", "calendar-once", &input, |s| {
            s.create_labor_leave_request(&dev, &input)
        })
        .unwrap();
    assert_eq!(request["id"], replay["id"]);
    assert!(f.store.create_labor_leave_request(&dev, &input).is_err());
    let id = request["id"].as_str().unwrap().to_owned();
    let decision = json!({"approved":true,"reason":"Approved"});
    assert!(f.store.decide_labor_request(&dev, &id, &decision).is_err());
    assert!(
        f.store
            .decide_labor_request(&outsider, &id, &decision)
            .is_err()
    );
    assert_eq!(
        f.store.labor_calendar(&other).unwrap()["requests"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        f.store.labor_calendar(&manager).unwrap()["requests"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // Separate PostgreSQL connections race over the same pending request.
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
            .decide_labor_request(&m1, &i1, &json!({"approved":true,"reason":"Approved"}))
            .is_ok()
    });
    let t2 = std::thread::spawn(move || {
        barrier.wait();
        third
            .decide_labor_request(&m2, &i2, &json!({"approved":false,"reason":"Rejected"}))
            .is_ok()
    });
    assert_ne!(t1.join().unwrap(), t2.join().unwrap());
    let profile = f.store.organization_profile(&f.ctx).unwrap();
    let country_change =
        serde_json::from_value(json!({"expected_revision":profile.revision,"country_code":"SV"}))
            .unwrap();
    f.store
        .change_organization_profile(&f.ctx, &country_change, None)
        .unwrap();
    let settings = f.store.labor_calendar_settings(&f.ctx).unwrap();
    assert_eq!(settings["country"], "SV");
    assert!(
        f.store
            .change_labor_calendar(
                &f.ctx,
                &json!({"expected_revision":settings["revision"],"country":"US"})
            )
            .is_err()
    );
    let holiday = f
        .store
        .propose_labor_holiday(
            &dev,
            &json!({"name":"Holiday","date":"2026-11-15","country":"US","kind":"public_holiday"}),
        )
        .unwrap();
    assert_eq!(holiday["country"], "SV");
    assert_eq!(holiday["kind"], "public_holiday");
    assert_eq!(holiday["reason"], "");
    let local = f
        .store
        .propose_labor_holiday(
            &f.ctx,
            &json!({"name":"Town festival","date":"2026-11-16","kind":"holiday","reason":""}),
        )
        .unwrap();
    assert_eq!(local["kind"], "holiday");
    assert_eq!(local["status"], "approved");
    assert!(
        f.store
            .decide_labor_holiday(&dev, holiday["id"].as_str().unwrap(), &decision)
            .is_err()
    );
    assert!(
        f.store
            .decide_labor_holiday(&manager, holiday["id"].as_str().unwrap(), &decision)
            .is_err()
    );
    f.store
        .decide_labor_holiday(&f.ctx, holiday["id"].as_str().unwrap(), &decision)
        .unwrap();
    let settings = f.store.labor_calendar_settings(&f.ctx).unwrap();
    let mut change = settings.clone();
    change["expected_revision"] = settings["revision"].clone();
    change["timezone"] = json!("Invalid/Moon");
    assert!(f.store.change_labor_calendar(&f.ctx, &change).is_err());
    change["timezone"] = json!("America/El_Salvador");
    assert!(f.store.change_labor_calendar(&dev, &change).is_err());
    f.store.change_labor_calendar(&f.ctx, &change).unwrap();
    assert!(f.store.change_labor_calendar(&f.ctx, &change).is_err());
    let mut invalid = input.clone();
    invalid["start_date"] = json!("2026-02-30");
    assert!(f.store.create_labor_leave_request(&dev, &invalid).is_err());
    invalid["start_date"] = json!("2020-01-01");
    invalid["end_date"] = json!("2026-01-01");
    assert!(f.store.create_labor_leave_request(&dev, &invalid).is_err());

    let api_key = f
        .store
        .issue_key(
            &f.ctx,
            dev.actor_id(),
            "Calendar key",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let api = f.store.authenticate(&api_key.key).unwrap();
    assert!(f.store.create_labor_leave_request(&api, &input).is_err());
    assert!(f.store.labor_calendar(&api).is_err());
    let password = zeroize::Zeroizing::new(SecretGenerator::token("session_test").unwrap());
    let actor = f
        .store
        .create_user(&f.ctx, "revoked", &password, "Revoked")
        .unwrap();
    let token = f.store.login("revoked", &password).unwrap();
    let revoked = f.store.authenticate(&token.access_token).unwrap();
    f.store.logout(&token.access_token).unwrap();
    assert!(f.store.labor_calendar(&revoked).is_err());
    assert!(!actor.id.is_empty());
    f.store
        .assign_labor_approver(
            &f.ctx,
            dev.actor_id(),
            &json!({"approver_id":null,"expected_revision":1}),
        )
        .unwrap();
    assert_eq!(
        f.store.labor_calendar(&manager).unwrap()["requests"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    f.store
        .assign_labor_approver(
            &f.ctx,
            dev.actor_id(),
            &json!({"approver_id":manager.actor_id(),"expected_revision":2}),
        )
        .unwrap();
    // Existing authenticated manager context loses authority after role scope changes.
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
        f.store.labor_calendar(&manager).unwrap()["requests"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}
