#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; transactional assignment and approval notifications, current authorization and durable delivery"]
fn email_events_are_durable_and_do_not_outlive_recipient_authority() {
    let mut f = WorkItemFixture::new();
    let actor = f.ctx.actor_id().to_owned();
    f.store
        .change_notification_recipient(
            &f.ctx,
            &actor,
            &NotificationRecipientChange {
                email: "owner@example.invalid".into(),
                assignments: true,
                approvals: true,
                expected_revision: 0,
            },
        )
        .unwrap();
    let flow=f.store.create_developer_workflow(&f.ctx,&serde_json::from_value(json!({"name":"Assignment flow","graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[]}]}})).unwrap()).unwrap();
    let published = f
        .store
        .publish_developer_workflow(
            &f.ctx,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Publish".into(),
            },
        )
        .unwrap();
    f.store
        .assign_actor_workflow(
            &f.ctx,
            &actor,
            &DeveloperWorkflowChange {
                expected_revision: 0,
                workflow_version_id: published.id.clone(),
                reason: "Join workflow".into(),
            },
        )
        .unwrap();
    let task = f.task("Assignment fixture");
    let task = f
        .store
        .change_task(
            &f.ctx,
            &f.project.id,
            &task.id,
            &TaskChange {
                expected_revision: task.revision,
                assignee_id: Some(actor.clone()),
                assignment_state_id: Some("ready".into()),
                assignment_workflow_version_id: Some(published.id),
                reason: "Assign".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let mail = f.store.claim_email_notification(true).unwrap().unwrap();
    assert_eq!(mail.kind, "assignment");
    assert_eq!(mail.resource_id, task.id);
    assert_eq!(mail.actor_id, actor);
    assert_eq!(mail.attempts, 1);
    assert!(f.store.claim_email_notification(true).unwrap().is_none());
    f.store
        .finish_email_notification(&mail.event_key, mail.attempts, "sent")
        .unwrap();
    f.store
        .change_task(
            &f.ctx,
            &f.project.id,
            &task.id,
            &TaskChange {
                expected_revision: task.revision,
                title: Some("A metadata edit".into()),
                reason: "Rename".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(f.store.claim_email_notification(true).unwrap().is_none());
    let workflow=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Email approval","purpose":"Notify authorized humans","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"human_approval"}]}]}})).unwrap()).unwrap();
    let flow = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &workflow.id,
            &WorkflowPublish {
                expected_revision: workflow.revision,
                reason: "Publish".into(),
            },
        )
        .unwrap();
    let task=f.store.create_task(&f.ctx,&f.project.id,&serde_json::from_value(json!({"title":"Approval notification","type_code":"task","workflow_version_id":flow.id,"origin":"fixture"})).unwrap()).unwrap();
    let definition=serde_json::from_value(json!({"target":{"kind":"task","task_id":task.id,"transition_id":"finish","parameters":{"expected_revision":task.revision,"reason":"Review"}},"reason":"Request review","expires_in_seconds":3600})).unwrap();
    let request = f
        .store
        .create_approval_request(&f.ctx, &f.project.id, &definition)
        .unwrap();
    let mail = f.store.claim_email_notification(true).unwrap().unwrap();
    assert_eq!(mail.kind, "approval");
    assert_eq!(mail.resource_id, request.id);
    f.store
        .finish_email_notification(&mail.event_key, mail.attempts, "sent")
        .unwrap();
    f.store
        .create_approval_request(&f.ctx, &f.project.id, &definition)
        .unwrap();
    f.store
        .revoke_grant(&f.ctx, &actor, Capability::ApprovalsGrant, None)
        .unwrap();
    assert!(f.store.claim_email_notification(true).unwrap().is_none());
}
