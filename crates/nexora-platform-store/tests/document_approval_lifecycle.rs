#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; document approval query/revocation, exact receipts and consumption rollback"]
fn document_approvals_are_inspectable_revocable_and_not_reusable() {
    let mut f = WorkItemFixture::new();
    f.task("Ordinary task policy remains separate");
    let flow=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Document authorization","purpose":"Human review can be withdrawn before execution","resource_kinds":["requirement"],"graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"reviewed","name":"Reviewed","publishable":true}],"transitions":[{"id":"review","name":"Review","from":"draft","to":"reviewed","guards":[{"kind":"human_approval"},{"kind":"reason"}]}]}})).unwrap()).unwrap();
    let flow = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Publish human review policy".into(),
            },
        )
        .unwrap();
    let document=f.store.create_project_document(&f.ctx,&f.project.id,&serde_json::from_value(json!({"code":"REQ-REVOKE","title":"Reviewed rule","content":{"kind":"requirement","body":{"problem":"Unclear rule","objective":"Explicit rule","scope":"Checkout","business_actors":["Customer"],"acceptance_criteria":["Declared criterion"],"source":"Business owner"}},"origin":"declared_business","workflow_version_id":flow.id,"reason":"Declare review source"})).unwrap()).unwrap();
    let parameters: DocumentTransitionRequest =
        serde_json::from_value(json!({"expected_state_revision":1,"reason":"Apply reviewed rule"}))
            .unwrap();
    let definition = ApprovalRequestCreate {
        target: ApprovalRequestTarget::DocumentVersion {
            document_id: document.id.clone(),
            version_id: document.latest_version_id.clone(),
            transition_id: "review".into(),
            parameters: parameters.clone(),
        },
        reason: "Ask reviewer".into(),
        expires_in_seconds: 3600,
    };
    let decision = ApprovalRequestDecide {
        expected_revision: 1,
        decision: ApprovalDecision::Approved,
        reason: "Human reviewed exact version".into(),
        approval_expires_in_seconds: 600,
    };
    let request = f
        .store
        .create_approval_request(&f.ctx, &f.project.id, &definition)
        .unwrap();
    let approved = f
        .store
        .decide_approval_request(&f.ctx, &f.project.id, &request.id, &decision)
        .unwrap();
    let id = approved.approval_id.as_deref().unwrap();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Human technical client",
            KeyScope {
                capabilities: vec![Capability::DocumentsRead, Capability::ApprovalsGrant],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let technical = f.store.authenticate(&key.key).unwrap();
    let read = f
        .store
        .get_document_approval(
            &technical,
            &f.project.id,
            &document.id,
            &document.latest_version_id,
            id,
        )
        .unwrap();
    assert_eq!(read.id, id);
    assert!(read.revoked_at.is_none());
    assert!(matches!(
        f.store
            .get_document_approval(&technical, &f.project.id, &document.id, "other-version", id),
        Err(PlatformError::NotFound)
    ));
    let revocation = DocumentApprovalRevoke {
        reason: "Withdraw approval before execution".into(),
    };
    assert!(matches!(
        f.store.revoke_document_approval(
            &technical,
            &f.project.id,
            &document.id,
            &document.latest_version_id,
            id,
            &revocation
        ),
        Err(PlatformError::Forbidden)
    ));
    let canonical = json!({"project_id":f.project.id,"document_id":document.id,"version_id":document.latest_version_id,"approval_id":id,"revocation":revocation});
    let revoked = f
        .store
        .request(
            &f.ctx,
            "document.approval_revoke",
            "revoke-once",
            &canonical,
            |store| {
                store.revoke_document_approval(
                    &f.ctx,
                    &f.project.id,
                    &document.id,
                    &document.latest_version_id,
                    id,
                    &revocation,
                )
            },
        )
        .unwrap();
    assert!(revoked.revoked_at.is_some());
    assert!(revoked.consumed_at.is_none());
    assert_eq!(
        f.store
            .request(
                &f.ctx,
                "document.approval_revoke",
                "revoke-once",
                &canonical,
                |store| store.revoke_document_approval(
                    &f.ctx,
                    &f.project.id,
                    &document.id,
                    &document.latest_version_id,
                    id,
                    &revocation
                )
            )
            .unwrap()
            .revoked_at,
        revoked.revoked_at
    );
    let mut rejected = parameters.clone();
    rejected.approval_id = Some(id.into());
    assert!(
        f.store
            .transition_document_version(
                &f.ctx,
                &f.project.id,
                &document.id,
                &document.latest_version_id,
                "review",
                &rejected
            )
            .is_err()
    );
    assert_eq!(
        f.store
            .get_approval_request(&f.ctx, &f.project.id, &request.id)
            .unwrap()
            .status,
        ApprovalRequestStatus::Approved
    );
    let request = f
        .store
        .create_approval_request(&f.ctx, &f.project.id, &definition)
        .unwrap();
    let approved = f
        .store
        .decide_approval_request(&f.ctx, &f.project.id, &request.id, &decision)
        .unwrap();
    let id = approved.approval_id.as_deref().unwrap();
    let mut operation = parameters;
    operation.approval_id = Some(id.into());
    let mut altered = operation.clone();
    altered.reason = "Different operation".into();
    assert!(
        f.store
            .transition_document_version(
                &f.ctx,
                &f.project.id,
                &document.id,
                &document.latest_version_id,
                "review",
                &altered
            )
            .is_err()
    );
    assert!(
        f.store
            .get_document_approval(
                &technical,
                &f.project.id,
                &document.id,
                &document.latest_version_id,
                id
            )
            .unwrap()
            .consumed_at
            .is_none()
    );
    let reviewed = f
        .store
        .transition_document_version(
            &f.ctx,
            &f.project.id,
            &document.id,
            &document.latest_version_id,
            "review",
            &operation,
        )
        .unwrap();
    assert_eq!(reviewed.state_id, "reviewed");
    assert!(
        f.store
            .get_document_approval(
                &technical,
                &f.project.id,
                &document.id,
                &document.latest_version_id,
                id
            )
            .unwrap()
            .consumed_at
            .is_some()
    );
    assert!(matches!(
        f.store.revoke_document_approval(
            &f.ctx,
            &f.project.id,
            &document.id,
            &document.latest_version_id,
            id,
            &revocation
        ),
        Err(PlatformError::Conflict(_))
    ));
    drop(f.store);
    let mut restarted =
        nexora_platform_store::PlatformStore::connect(&f.config, &f.namespace).unwrap();
    assert!(
        restarted
            .get_document_approval(
                &technical,
                &f.project.id,
                &document.id,
                &document.latest_version_id,
                id
            )
            .unwrap()
            .consumed_at
            .is_some()
    );
}
