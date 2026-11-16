#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; human-account approval decisions for exact task/document operations requested by external actors"]
fn approval_requests_need_human_sessions_and_bind_exact_resource_operations() {
    let mut f = WorkItemFixture::new();
    f.task("Ordinary task fixtures retain their flow");
    let actor = f
        .store
        .create_actor(
            &f.ctx,
            ActorKind::Agent,
            "External requester",
            "Request and execute configured approved operations",
        )
        .unwrap();
    let capabilities = vec![
        Capability::TasksRead,
        Capability::TasksWrite,
        Capability::TasksTransition,
        Capability::DocumentsRead,
        Capability::DocumentsWrite,
        Capability::DocumentsPublish,
        Capability::ApprovalsGrant,
    ];
    for capability in &capabilities {
        f.store
            .grant(&f.ctx, &actor.id, *capability, Some(&f.project.id))
            .unwrap();
    }
    let key = f
        .store
        .issue_key(
            &f.ctx,
            &actor.id,
            "Approval requester",
            KeyScope {
                capabilities,
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let agent = f.store.authenticate(&key.key).unwrap();
    let flow=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Human delivery","purpose":"Explicit account approval condition","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"reason"},{"kind":"human_approval"}]}]}})).unwrap()).unwrap();
    let version = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Require exact human approval".into(),
            },
        )
        .unwrap();
    let task=f.store.create_task(&agent,&f.project.id,&serde_json::from_value(json!({"title":"Reviewed delivery","type_code":"task","workflow_version_id":version.id,"origin":"External requester"})).unwrap()).unwrap();
    let parameters =
        json!({"expected_revision":task.revision,"reason":"Deliver exact reviewed change"});
    let definition=serde_json::from_value(json!({"target":{"kind":"task","task_id":task.id,"transition_id":"finish","parameters":parameters},"reason":"Request configured human authorization","expires_in_seconds":3600})).unwrap();
    let body = json!({"project_id":f.project.id,"definition":definition});
    let request = f
        .store
        .request(
            &agent,
            "approval_request.create",
            "request-once",
            &body,
            |store| store.create_approval_request(&agent, &f.project.id, &definition),
        )
        .unwrap();
    let replay = f
        .store
        .request(
            &agent,
            "approval_request.create",
            "request-once",
            &body,
            |store| store.create_approval_request(&agent, &f.project.id, &definition),
        )
        .unwrap();
    assert_eq!(request.id, replay.id);
    assert_eq!(request.requested_by, actor.id);
    assert_eq!(serde_json::to_value(&request).unwrap()["status"], "pending");
    let decision=serde_json::from_value(json!({"expected_revision":request.revision,"decision":"approved","reason":"Reviewed by human account","approval_expires_in_seconds":600})).unwrap();
    assert!(matches!(
        f.store
            .decide_approval_request(&agent, &f.project.id, &request.id, &decision),
        Err(PlatformError::Forbidden)
    ));
    let human_key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Human account technical key",
            KeyScope {
                capabilities: vec![
                    Capability::ApprovalsGrant,
                    Capability::TasksRead,
                    Capability::DocumentsRead,
                ],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let human_key_ctx = f.store.authenticate(&human_key.key).unwrap();
    assert!(matches!(
        f.store
            .decide_approval_request(&human_key_ctx, &f.project.id, &request.id, &decision),
        Err(PlatformError::Forbidden)
    ));
    let approved = f
        .store
        .decide_approval_request(&f.ctx, &f.project.id, &request.id, &decision)
        .unwrap();
    assert_eq!(
        serde_json::to_value(&approved).unwrap()["status"],
        "approved"
    );
    let approval_id = approved.approval_id.as_ref().unwrap();
    let completed=f.store.transition_task(&agent,&f.project.id,&task.id,"finish",&serde_json::from_value(json!({"expected_revision":task.revision,"reason":"Deliver exact reviewed change","approval_id":approval_id})).unwrap()).unwrap();
    assert_eq!(completed.state_id.as_deref(), Some("done"));
    assert!(
        f.store
            .decide_approval_request(&f.ctx, &f.project.id, &request.id, &decision)
            .is_err()
    );
    let rejected_task=f.store.create_task(&agent,&f.project.id,&serde_json::from_value(json!({"title":"Rejected proposal","type_code":"task","workflow_version_id":version.id,"origin":"External requester"})).unwrap()).unwrap();
    let pending=f.store.create_approval_request(&agent,&f.project.id,&serde_json::from_value(json!({"target":{"kind":"task","task_id":rejected_task.id,"transition_id":"finish","parameters":{"expected_revision":rejected_task.revision,"reason":"Proposed delivery"}},"reason":"Ask for review","expires_in_seconds":3600})).unwrap()).unwrap();
    let rejected=f.store.decide_approval_request(&f.ctx,&f.project.id,&pending.id,&serde_json::from_value(json!({"expected_revision":pending.revision,"decision":"rejected","reason":"Proposal needs changes","approval_expires_in_seconds":600})).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&rejected).unwrap()["status"],
        "rejected"
    );
    assert!(rejected.approval_id.is_none());
    let pending=f.store.create_approval_request(&agent,&f.project.id,&serde_json::from_value(json!({"target":{"kind":"task","task_id":rejected_task.id,"transition_id":"finish","parameters":{"expected_revision":rejected_task.revision,"reason":"Withdraw this proposal"}},"reason":"Another proposal","expires_in_seconds":3600})).unwrap()).unwrap();
    let cancelled=f.store.cancel_approval_request(&agent,&f.project.id,&pending.id,&serde_json::from_value(json!({"expected_revision":pending.revision,"reason":"Requester withdrew proposal"})).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&cancelled).unwrap()["status"],
        "cancelled"
    );
    let pending=f.store.create_approval_request(&agent,&f.project.id,&serde_json::from_value(json!({"target":{"kind":"task","task_id":rejected_task.id,"transition_id":"finish","parameters":{"expected_revision":rejected_task.revision,"reason":"Review before resource changes"}},"reason":"Resource revision is bound","expires_in_seconds":3600})).unwrap()).unwrap();
    f.store.change_task(&agent,&f.project.id,&rejected_task.id,&serde_json::from_value(json!({"expected_revision":rejected_task.revision,"title":"Changed proposal","reason":"Invalidate the requested operation"})).unwrap()).unwrap();
    assert!(f.store.decide_approval_request(&f.ctx,&f.project.id,&pending.id,&serde_json::from_value(json!({"expected_revision":pending.revision,"decision":"approved","reason":"Cannot approve a different task revision","approval_expires_in_seconds":600})).unwrap()).is_err());

    let document_flow=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Human business review","purpose":"Same configured human condition for document versions","resource_kinds":["requirement"],"graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"reviewed","name":"Reviewed","publishable":true}],"transitions":[{"id":"review","name":"Review","from":"draft","to":"reviewed","guards":[{"kind":"reason"},{"kind":"human_approval"}]}]}})).unwrap()).unwrap();
    let flow = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &document_flow.id,
            &WorkflowPublish {
                expected_revision: document_flow.revision,
                reason: "Business review policy".into(),
            },
        )
        .unwrap();
    let document=f.store.create_project_document(&agent,&f.project.id,&serde_json::from_value(json!({"code":"REQ-HUMAN","title":"Human reviewed requirement","content":{"kind":"requirement","body":{"problem":"Business policy requires review","objective":"Record reviewed conditions","scope":"Checkout","business_actors":["Customer"],"acceptance_criteria":["Conditions are explicit"],"source":"External proposal"}},"origin":"agent_proposed","workflow_version_id":flow.id,"reason":"Propose business requirement"})).unwrap()).unwrap();
    assert!(
        f.store
            .transition_document_version(
                &agent,
                &f.project.id,
                &document.id,
                &document.latest_version_id,
                "review",
                &serde_json::from_value(
                    json!({"expected_state_revision":1,"reason":"Review exact requirement"})
                )
                .unwrap()
            )
            .is_err()
    );
    let request=f.store.create_approval_request(&agent,&f.project.id,&serde_json::from_value(json!({"target":{"kind":"document_version","document_id":document.id,"version_id":document.latest_version_id,"transition_id":"review","parameters":{"expected_state_revision":1,"reason":"Review exact requirement"}},"reason":"Request human business review","expires_in_seconds":3600})).unwrap()).unwrap();
    let decision=serde_json::from_value(json!({"expected_revision":request.revision,"decision":"approved","reason":"Human reviewed proposed requirement","approval_expires_in_seconds":600})).unwrap();
    assert!(
        f.store
            .decide_approval_request(&agent, &f.project.id, &request.id, &decision)
            .is_err()
    );
    let approved = f
        .store
        .decide_approval_request(&f.ctx, &f.project.id, &request.id, &decision)
        .unwrap();
    let parameters=serde_json::from_value(json!({"expected_state_revision":1,"reason":"Review exact requirement","approval_id":approved.approval_id})).unwrap();
    let reviewed = f
        .store
        .transition_document_version(
            &agent,
            &f.project.id,
            &document.id,
            &document.latest_version_id,
            "review",
            &parameters,
        )
        .unwrap();
    assert_eq!(reviewed.state_id, "reviewed");
    assert!(
        f.store
            .transition_document_version(
                &agent,
                &f.project.id,
                &document.id,
                &document.latest_version_id,
                "review",
                &parameters
            )
            .is_err()
    );
    f.store
        .publish_project_document(
            &agent,
            &f.project.id,
            &document.id,
            &ProjectDocumentPublish {
                expected_revision: document.revision,
                version_id: reviewed.id,
                expected_state_revision: reviewed.state_revision,
                reason: "Publish human-reviewed version".into(),
            },
        )
        .unwrap();
    let page = f
        .store
        .approval_request_page(&agent, &f.project.id, None, 100, None)
        .unwrap();
    assert_eq!(page.items.len(), 5);
    let read = f
        .store
        .get_approval_request(&agent, &f.project.id, &request.id)
        .unwrap();
    assert_eq!(read.approval_id, approved.approval_id);
    assert!(matches!(
        f.store
            .get_approval_request(&agent, "foreign-project", &request.id),
        Err(PlatformError::NotFound)
    ));
    drop(f.store);
    let mut restarted =
        nexora_platform_store::PlatformStore::connect(&f.config, &f.namespace).unwrap();
    assert_eq!(
        restarted
            .get_approval_request(&agent, &f.project.id, &request.id)
            .unwrap()
            .approval_id,
        approved.approval_id
    );
    restarted.revoke_key(&f.ctx, &key.metadata.id).unwrap();
    assert!(matches!(
        restarted.get_approval_request(&agent, &f.project.id, &request.id),
        Err(PlatformError::Unauthorized)
    ));
}
