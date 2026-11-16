#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use serde_json::json;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires local PostgreSQL"]
fn vacation_without_reason_is_persisted_but_leave_still_requires_one() {
    let mut fixture = WorkItemFixture::new();
    let input = json!({"kind":"vacation","start_date":"2027-02-01","end_date":"2027-02-02"});
    let created = fixture
        .store
        .create_labor_leave_request(&fixture.ctx, &input)
        .unwrap();
    assert_eq!(created["reason"], "");
    assert_eq!(created["status"], "pending");
    let leave = json!({"kind":"leave","start_date":"2027-03-01","end_date":"2027-03-01"});
    assert!(
        fixture
            .store
            .create_labor_leave_request(&fixture.ctx, &leave)
            .is_err()
    );
}
