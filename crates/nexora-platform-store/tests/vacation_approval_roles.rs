#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::{AuthContext, MemberRole, MemberRoleChange, PlatformError};
use nexora_platform_store::SecretGenerator;
use serde_json::json;
use work_item_fixture::WorkItemFixture;

fn member(fixture: &mut WorkItemFixture, name: &str, role: MemberRole) -> AuthContext {
    let password = zeroize::Zeroizing::new(SecretGenerator::token("vacation_test").unwrap());
    let actor = fixture
        .store
        .create_user(&fixture.ctx, name, &password, name)
        .unwrap();
    fixture
        .store
        .change_member_role(
            &fixture.ctx,
            &actor.id,
            &MemberRoleChange {
                role,
                all_projects: true,
                project_ids: vec![],
                expected_revision: 0,
            },
        )
        .unwrap();
    let session = fixture.store.login(name, &password).unwrap();
    let verified = fixture
        .store
        .identity_gate(&session.access_token, "skip", "")
        .unwrap();
    fixture
        .store
        .authenticate(verified["access_token"].as_str().unwrap())
        .unwrap()
}

#[test]
#[ignore = "requires local PostgreSQL"]
fn vacation_decisions_require_manager_or_superadmin_and_forbid_self_approval() {
    let mut f = WorkItemFixture::new();
    let developer = member(&mut f, "developer", MemberRole::Developer);
    let lead = member(&mut f, "lead", MemberRole::TechnicalLead);
    let pmo = member(&mut f, "pmo", MemberRole::Pmo);
    let manager = member(&mut f, "manager", MemberRole::Manager);
    f.store
        .assign_labor_approver(
            &f.ctx,
            developer.actor_id(),
            &json!({"approver_id":lead.actor_id(),"expected_revision":0}),
        )
        .unwrap();
    let vacation = f
        .store
        .create_labor_leave_request(
            &developer,
            &json!({"kind":"vacation","start_date":"2027-04-01","end_date":"2027-04-02"}),
        )
        .unwrap();
    let id = vacation["id"].as_str().unwrap();
    for reviewer in [&developer, &lead, &pmo] {
        let calendar = f.store.labor_calendar(reviewer).unwrap();
        let projected = calendar["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id);
        assert!(!projected.is_some_and(|row| row["can_decide"] == true));
        for approved in [true, false] {
            assert!(matches!(
                f.store.decide_labor_request(
                    reviewer,
                    id,
                    &json!({"approved":approved,"reason":"Decision"})
                ),
                Err(PlatformError::Forbidden)
            ));
        }
    }
    for reviewer in [&manager, &f.ctx] {
        let calendar = f.store.labor_calendar(reviewer).unwrap();
        assert_eq!(
            calendar["requests"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == id)
                .unwrap()["can_decide"],
            true
        );
    }
    assert_eq!(
        f.store
            .decide_labor_request(&manager, id, &json!({"approved":true,"reason":"Approved"}))
            .unwrap()["status"],
        "approved"
    );
    let own = f
        .store
        .create_labor_leave_request(
            &manager,
            &json!({"kind":"vacation","start_date":"2027-05-01","end_date":"2027-05-02"}),
        )
        .unwrap();
    let own_id = own["id"].as_str().unwrap();
    assert!(matches!(
        f.store
            .decide_labor_request(&manager, own_id, &json!({"approved":true,"reason":"Own"})),
        Err(PlatformError::Forbidden)
    ));
    assert_eq!(
        f.store
            .decide_labor_request(
                &f.ctx,
                own_id,
                &json!({"approved":true,"reason":"Approved"})
            )
            .unwrap()["status"],
        "approved"
    );
    let leave = f.store.create_labor_leave_request(&developer, &json!({"kind":"leave","start_date":"2027-06-01","end_date":"2027-06-01","reason":"Appointment"})).unwrap();
    assert_eq!(
        f.store
            .decide_labor_request(
                &lead,
                leave["id"].as_str().unwrap(),
                &json!({"approved":true,"reason":"Approved"})
            )
            .unwrap()["status"],
        "approved"
    );
}
