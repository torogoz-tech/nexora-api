#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::{WorkflowChange, WorkflowCreate};
use serde_json::json;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; workflow display color persists independently of state colors"]
fn workflow_color_round_trips_and_can_be_cleared() {
    let mut f = WorkItemFixture::new();
    let input: WorkflowCreate = serde_json::from_value(json!({"name":"Colored workflow","color":"#aAbBcC","resource_kinds":["task"],"graph":{"states":[{"id":"new","name":"New","initial":true,"color":"#123456"}],"transitions":[]}})).unwrap();
    let created = f
        .store
        .create_workflow(&f.ctx, &f.project.id, &input)
        .unwrap();
    assert_eq!(created.color.as_deref(), Some("#AABBCC"));
    let current = f
        .store
        .get_workflow(&f.ctx, &f.project.id, &created.id)
        .unwrap();
    assert_eq!(current.color, created.color);
    let invalid = f.store.change_workflow(
        &f.ctx,
        &f.project.id,
        &created.id,
        &WorkflowChange {
            expected_revision: created.revision,
            color: Some("red;bad".into()),
            reason: "Invalid color".into(),
            ..Default::default()
        },
    );
    assert!(invalid.is_err());
    let cleared = f
        .store
        .change_workflow(
            &f.ctx,
            &f.project.id,
            &created.id,
            &WorkflowChange {
                expected_revision: created.revision,
                color: Some(String::new()),
                reason: "Use theme color".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(cleared.color.is_none());
    assert_eq!(
        serde_json::to_value(cleared.draft).unwrap()["states"][0]["color"],
        "#123456"
    );
}
