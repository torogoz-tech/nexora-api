#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::PlatformStore;
use serde_json::json;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; external actors contribute typed immutable requirement revisions governed by configured publication states"]
fn project_requirements_keep_history_and_publish_only_through_their_configured_flow() {
    let mut f = WorkItemFixture::new();
    let task = f.task("Document linked business change");
    let workflow=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Requirement review","purpose":"Configured publication eligibility","resource_kinds":["requirement"],"graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"reviewed","name":"Reviewed","terminal":true,"publishable":true}],"transitions":[{"id":"review","name":"Review","from":"draft","to":"reviewed","guards":[{"kind":"reason"}]}]}})).unwrap()).unwrap();
    let flow = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &workflow.id,
            &WorkflowPublish {
                expected_revision: workflow.revision,
                reason: "Configured document policy".into(),
            },
        )
        .unwrap();
    let actor = f
        .store
        .create_actor(
            &f.ctx,
            ActorKind::Agent,
            "External developer",
            "Contribute business documentation",
        )
        .unwrap();
    let capabilities = vec![
        Capability::DocumentsRead,
        Capability::DocumentsWrite,
        Capability::DocumentsPublish,
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
            "Documentation actor",
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
    let content = json!({"kind":"requirement","body":{"problem":"Discount eligibility is unclear","objective":"Make eligibility explicit","scope":"Customer ticket checkout","business_actors":["Customer","Cashier"],"acceptance_criteria":["The documented condition is testable"],"facts":[],"assumptions":["Source provided by business"],"decisions":[],"open_questions":["How are exceptions approved?"],"source":"Stakeholder interview"}});
    let input=serde_json::from_value(json!({"code":"REQ-001","title":"Discount eligibility","content":content,"origin":"declared_business","workflow_version_id":flow.id,"reason":"Initial requirement from external contributor"})).unwrap();
    let document = f
        .store
        .create_project_document(&agent, &f.project.id, &input)
        .unwrap();
    let first = f
        .store
        .document_version(
            &agent,
            &f.project.id,
            &document.id,
            &document.latest_version_id,
        )
        .unwrap();
    assert_eq!(first.state_id, "draft");
    assert_eq!(first.created_by, actor.id);
    assert!(f.store.publish_project_document(&agent,&f.project.id,&document.id,&serde_json::from_value(json!({"expected_revision":document.revision,"version_id":first.id,"expected_state_revision":first.state_revision,"reason":"Draft state is not publishable"})).unwrap()).is_err());
    let reviewed=f.store.transition_document_version(&agent,&f.project.id,&document.id,&first.id,"review",&serde_json::from_value(json!({"expected_state_revision":first.state_revision,"reason":"Review completed under this flow"})).unwrap()).unwrap();
    assert_eq!(reviewed.state_id, "reviewed");
    let publication=f.store.publish_project_document(&agent,&f.project.id,&document.id,&serde_json::from_value(json!({"expected_revision":document.revision,"version_id":first.id,"expected_state_revision":reviewed.state_revision,"reason":"Publish reviewed requirement"})).unwrap()).unwrap();
    assert_eq!(publication.version_id, first.id);
    assert_eq!(publication.checksum, first.checksum);
    let current = f
        .store
        .get_project_document(&agent, &f.project.id, &document.id)
        .unwrap();
    let mut next_content = content;
    next_content["body"]["objective"] =
        json!("Clarify exceptions without rewriting published history");
    let second=f.store.revise_project_document(&agent,&f.project.id,&document.id,&serde_json::from_value(json!({"expected_revision":current.revision,"title":"Discount eligibility revised","content":next_content,"origin":"agent_proposed","workflow_version_id":flow.id,"reason":"Propose clearer exceptions"})).unwrap()).unwrap();
    assert_eq!(second.number, 2);
    assert_eq!(second.state_id, "draft");
    let historical = f
        .store
        .document_version(&agent, &f.project.id, &document.id, &first.id)
        .unwrap();
    assert_eq!(historical.checksum, first.checksum);
    assert_eq!(
        historical.content["body"]["objective"],
        "Make eligibility explicit"
    );
    let current = f
        .store
        .get_project_document(&agent, &f.project.id, &document.id)
        .unwrap();
    assert_eq!(
        current.published_version_id.as_deref(),
        Some(first.id.as_str())
    );
    assert_eq!(current.latest_version_id, second.id);
    let page = f
        .store
        .document_version_page(&agent, &f.project.id, &document.id, None, 1)
        .unwrap();
    assert_eq!(page.items[0].id, second.id);
    assert!(page.next_before.is_some());
    assert!(matches!(
        f.store
            .get_project_document(&agent, "other-project", &document.id),
        Err(PlatformError::NotFound)
    ));
    let json_export = f
        .store
        .export_project_document(&agent, &f.project.id, &document.id, &first.id, "json")
        .unwrap();
    assert!(json_export.content.contains("Make eligibility explicit"));
    let markdown = f
        .store
        .export_project_document(&agent, &f.project.id, &document.id, &first.id, "markdown")
        .unwrap();
    assert!(markdown.content.contains("Discount eligibility"));
    assert!(markdown.content.contains(&first.checksum));
    let current = f
        .store
        .get_project_document(&agent, &f.project.id, &document.id)
        .unwrap();
    f.store.change_project_document(&agent,&f.project.id,&document.id,&serde_json::from_value(json!({"expected_revision":current.revision,"archived":true,"reason":"Retain historical requirement"})).unwrap()).unwrap();
    assert_eq!(
        f.store
            .document_version(&agent, &f.project.id, &document.id, &first.id)
            .unwrap()
            .checksum,
        first.checksum
    );
    drop(f.store);
    let mut store = PlatformStore::connect(&f.config, &f.namespace).unwrap();
    assert_eq!(
        store
            .document_version(&agent, &f.project.id, &document.id, &first.id)
            .unwrap()
            .checksum,
        first.checksum
    );
    store.revoke_key(&f.ctx, &key.metadata.id).unwrap();
    assert!(matches!(
        store.get_project_document(&agent, &f.project.id, &document.id),
        Err(PlatformError::Unauthorized)
    ));
    // The task fixture remains a normal project resource; no documentation is inferred from it.
    assert_eq!(
        store
            .get_task(&f.ctx, &f.project.id, &task.id)
            .unwrap()
            .title,
        "Document linked business change"
    );
}
