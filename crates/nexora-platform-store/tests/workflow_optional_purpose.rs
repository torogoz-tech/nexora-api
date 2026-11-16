#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::{DomainText, WorkflowChange, WorkflowCreate, WorkflowPublish};
use serde_json::json;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; optional workflow purposes persist in drafts and immutable versions"]
fn workflow_purpose_can_be_omitted_changed_and_cleared_before_publication() {
    let mut f = WorkItemFixture::new();
    let input: WorkflowCreate = serde_json::from_value(json!({
        "name":"Optional purpose",
        "resource_kinds":["task"],
        "graph": {
            "states":[{"id":"new","name":"New","initial":true},{"id":"done","name":"Done","terminal":true}],
            "transitions":[{"id":"finish","name":"Finish","from":"new","to":"done","guards":[]}]
        }
    })).unwrap();
    let created = f
        .store
        .create_workflow(&f.ctx, &f.project.id, &input)
        .unwrap();
    assert!(created.purpose.is_empty());
    let named = f
        .store
        .change_workflow(
            &f.ctx,
            &f.project.id,
            &created.id,
            &WorkflowChange {
                expected_revision: created.revision,
                purpose: Some("Optional explanation".into()),
                reason: "Add context".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let first = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &created.id,
            &WorkflowPublish {
                expected_revision: named.revision,
                reason: "Publish explanation".into(),
            },
        )
        .unwrap();
    let current = f
        .store
        .get_workflow(&f.ctx, &f.project.id, &created.id)
        .unwrap();
    let cleared = f
        .store
        .change_workflow(
            &f.ctx,
            &f.project.id,
            &created.id,
            &WorkflowChange {
                expected_revision: current.revision,
                purpose: Some(" \n\t ".into()),
                reason: "Remove optional explanation".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(cleared.purpose.is_empty());
    let published = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &created.id,
            &WorkflowPublish {
                expected_revision: cleared.revision,
                reason: "Publish without explanation".into(),
            },
        )
        .unwrap();
    assert!(published.purpose.is_empty());
    assert_eq!(
        f.store
            .workflow_version(&f.ctx, &f.project.id, &created.id, &first.id)
            .unwrap()
            .purpose,
        "Optional explanation"
    );
    assert!(DomainText::optional(&"a".repeat(4001), 4000).is_err());
    assert!(DomainText::optional("invalid\0text", 4000).is_err());
}
