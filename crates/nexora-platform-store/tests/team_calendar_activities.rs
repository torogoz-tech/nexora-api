#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::SecretGenerator;
use serde_json::{Value, json};
use work_item_fixture::WorkItemFixture;
fn member(f: &mut WorkItemFixture, name: &str, role: MemberRole) -> AuthContext {
    let password = zeroize::Zeroizing::new(SecretGenerator::token("calendar_test").unwrap());
    let actor = f.store.create_user(&f.ctx, name, &password, name).unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &actor.id,
            &MemberRoleChange {
                role,
                all_projects: true,
                project_ids: vec![],
                expected_revision: 0,
            },
        )
        .unwrap();
    let token = f.store.login(name, &password).unwrap();
    let verified = f
        .store
        .identity_gate(&token.access_token, "skip", "")
        .unwrap();
    f.store
        .authenticate(verified["access_token"].as_str().unwrap())
        .unwrap()
}
fn activity() -> Value {
    json!({"title":"Team planning","description":"Shared agenda","location":"Meeting room","start_date":"2026-11-10","end_date":"2026-11-10","all_day":false,"start_time":"09:00","end_time":"10:00"})
}
#[test]
#[ignore = "requires PostgreSQL"]
fn team_calendar_roles_visibility_revisions_and_self_requests() {
    let mut f = WorkItemFixture::new();
    let dev = member(&mut f, "dev", MemberRole::Developer);
    let pmo = member(&mut f, "pmo", MemberRole::Pmo);
    let manager = member(&mut f, "manager", MemberRole::Manager);
    let lead = member(&mut f, "lead", MemberRole::TechnicalLead);
    assert!(matches!(
        f.store
            .save_calendar_activity(&dev, None, &activity(), false),
        Err(PlatformError::Forbidden)
    ));
    for actor in [&f.ctx.clone(), &pmo, &manager, &lead] {
        let created = f
            .store
            .save_calendar_activity(actor, None, &activity(), false)
            .unwrap();
        let id = created["id"].as_str().unwrap();
        let visible = f
            .store
            .calendar_activities(&dev, "2026-11-01", "2026-11-30", None, 100)
            .unwrap();
        assert!(
            visible["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["id"] == id && v["can_edit"] == false)
        );
        let mut update = activity();
        update["expected_revision"] = json!(0);
        update["title"] = json!("Changed planning");
        if actor.actor_id() != pmo.actor_id() {
            assert!(matches!(
                f.store
                    .save_calendar_activity(&pmo, Some(id), &update, false),
                Err(PlatformError::Forbidden)
            ));
        }
        let changed = f
            .store
            .save_calendar_activity(actor, Some(id), &update, false)
            .unwrap();
        assert_eq!(changed["revision"], 1);
        assert!(matches!(
            f.store
                .save_calendar_activity(actor, Some(id), &update, false),
            Err(PlatformError::Conflict(_))
        ));
        f.store
            .save_calendar_activity(&f.ctx, Some(id), &json!({"expected_revision":1}), true)
            .unwrap();
        assert!(
            !f.store
                .calendar_activities(&dev, "2026-11-01", "2026-11-30", None, 100)
                .unwrap()["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["id"] == id)
        );
        let leave = json!({"kind":"leave","start_date":"2026-12-10","end_date":"2026-12-10","reason":"Personal leave","minutes_per_day":60,"actor_id":dev.actor_id()});
        let request = f.store.create_labor_leave_request(actor, &leave).unwrap();
        assert_eq!(request["actor_id"], actor.actor_id());
        assert!(
            f.store.labor_calendar(actor).unwrap()["requests"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["id"] == request["id"])
        );
        assert!(matches!(
            f.store.decide_labor_request(
                actor,
                request["id"].as_str().unwrap(),
                &json!({"approved":true,"reason":"Self"})
            ),
            Err(PlatformError::Forbidden)
        ));
        assert!(
            !f.store.labor_calendar(&dev).unwrap()["requests"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["id"] == request["id"])
        );
    }
    for invalid in [
        json!({"end_time":"08:00"}),
        json!({"start_time":"25:00"}),
        json!({"end_date":"2026-02-30"}),
        json!({"created_by":dev.actor_id()}),
        json!({"end_date":"2028-11-10"}),
    ] {
        let mut input = activity();
        for (k, v) in invalid.as_object().unwrap() {
            input[k] = v.clone();
        }
        assert!(matches!(
            f.store.save_calendar_activity(&pmo, None, &input, false),
            Err(PlatformError::Validation(_))
        ));
    }
    assert!(
        f.store
            .calendar_activities(&dev, "2026-01-01", "2026-12-31", None, 100)
            .is_err()
    );
    let mut input = activity();
    input["all_day"] = json!(true);
    input["start_time"] = Value::Null;
    input["end_time"] = Value::Null;
    let a = f
        .store
        .save_calendar_activity(&pmo, None, &input, false)
        .unwrap();
    let b = f
        .store
        .save_calendar_activity(&pmo, None, &input, false)
        .unwrap();
    let page = f
        .store
        .calendar_activities(&dev, "2026-11-01", "2026-11-30", None, 1)
        .unwrap();
    assert_eq!(page["has_more"], true);
    let next = f
        .store
        .calendar_activities(
            &dev,
            "2026-11-01",
            "2026-11-30",
            page["next_after"].as_str(),
            1,
        )
        .unwrap();
    assert_ne!(page["items"][0]["id"], next["items"][0]["id"]);
    assert_eq!(next["has_more"], false);
    assert_ne!(a["id"], b["id"]);
}

#[test]
#[ignore = "requires PostgreSQL"]
fn attendance_is_personal_revision_bound_and_approved_leave_is_public_without_reason() {
    let mut f = WorkItemFixture::new();
    let dev = member(&mut f, "attendee", MemberRole::Developer);
    let other = member(&mut f, "observer", MemberRole::Developer);
    let manager = member(&mut f, "organizer", MemberRole::Manager);
    let mut input = activity();
    input["mode"] = json!("virtual");
    input["location"] = json!("https://meeting.example.test/team");
    input["attendance_required"] = json!(true);
    let created = f
        .store
        .save_calendar_activity(&manager, None, &input, false)
        .unwrap();
    let id = created["id"].as_str().unwrap();
    f.store
        .respond_calendar_activity(
            &dev,
            id,
            &json!({"expected_revision":0,"response":"attending"}),
        )
        .unwrap();
    let detail = f.store.calendar_activity(&other, id).unwrap();
    assert_eq!(detail["participants"][0]["actor_id"], dev.actor_id());
    assert_eq!(detail["attending_count"], 1);
    assert_eq!(detail["my_attendance"], Value::Null);
    let page = f
        .store
        .calendar_activities(&dev, "2026-11-01", "2026-11-30", None, 100)
        .unwrap();
    assert_eq!(page["items"][0]["my_attendance"], "attending");
    assert_eq!(page["items"][0]["attending_count"], 1);
    let page = f
        .store
        .calendar_activities(&other, "2026-11-01", "2026-11-30", None, 100)
        .unwrap();
    assert_eq!(page["items"][0]["my_attendance"], Value::Null);
    assert!(
        f.store
            .respond_calendar_activity(
                &dev,
                id,
                &json!({"expected_revision":0,"response":"attending","actor_id":other.actor_id()})
            )
            .is_err()
    );
    input["expected_revision"] = json!(0);
    f.store
        .save_calendar_activity(&manager, Some(id), &input, false)
        .unwrap();
    assert!(
        f.store
            .respond_calendar_activity(
                &dev,
                id,
                &json!({"expected_revision":0,"response":"attending"})
            )
            .is_err()
    );
    let page = f
        .store
        .calendar_activities(&dev, "2026-11-01", "2026-11-30", None, 100)
        .unwrap();
    assert_eq!(page["items"][0]["my_attendance"], Value::Null);
    assert_eq!(page["items"][0]["attending_count"], 0);
    let leave=f.store.create_labor_leave_request(&dev,&json!({"kind":"leave","start_date":"2026-12-10","end_date":"2026-12-10","minutes_per_day":60,"reason":"Confidential medical detail"})).unwrap();
    let id = leave["id"].as_str().unwrap();
    assert!(
        !f.store.labor_calendar(&other).unwrap()["requests"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == id)
    );
    f.store
        .decide_labor_request(
            &manager,
            id,
            &json!({"approved":true,"reason":"Confidential decision"}),
        )
        .unwrap();
    let public = f.store.labor_calendar(&other).unwrap();
    let shared = public["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap();
    assert_eq!(shared["actor_id"], dev.actor_id());
    assert!(shared["reason"].as_str().is_none_or(str::is_empty));
    assert!(shared["decision_reason"].as_str().is_none_or(str::is_empty));
}
