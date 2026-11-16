#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; authorized business context catalog and auditable publication history"]
fn document_catalog_exposes_bounded_metadata_and_authored_history() {
    let mut f = WorkItemFixture::new();
    f.task("Tasks do not infer business documentation");
    let empty = f
        .store
        .project_document_page(
            &f.ctx,
            &f.project.id,
            &serde_json::from_value(json!({"limit":1})).unwrap(),
        )
        .unwrap();
    assert!(empty.items.is_empty());
    let definition=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Catalog policy","purpose":"Explicit publication","resource_kinds":["requirement"],"graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"reviewed","name":"Reviewed","publishable":true}],"transitions":[{"id":"review","name":"Review","from":"draft","to":"reviewed","guards":[{"kind":"reason"}]}]}})).unwrap()).unwrap();
    let flow = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &definition.id,
            &WorkflowPublish {
                expected_revision: definition.revision,
                reason: "Catalog fixture".into(),
            },
        )
        .unwrap();
    let content = json!({"kind":"requirement","body":{"problem":"Unclear policy","objective":"Explicit policy","scope":"Checkout","business_actors":["Customer"],"acceptance_criteria":["Testable condition"],"source":"Business interview"}});
    let mut ids = Vec::new();
    for index in 0..3 {
        let doc=f.store.create_project_document(&f.ctx,&f.project.id,&serde_json::from_value(json!({"code":format!("REQ-{index}"),"title":format!("Business requirement {index}"),"content":content,"origin":"declared_business","workflow_version_id":flow.id,"reason":"Record interview"})).unwrap()).unwrap();
        ids.push(doc.id);
    }
    ids.sort();
    let first = f
        .store
        .project_document_page(
            &f.ctx,
            &f.project.id,
            &serde_json::from_value(json!({"limit":1,"kind":"requirement"})).unwrap(),
        )
        .unwrap();
    assert_eq!(first.items[0].id, ids[0]);
    assert!(
        first.items[0]
            .latest_title
            .starts_with("Business requirement")
    );
    let metadata = serde_json::to_value(&first.items[0]).unwrap();
    assert!(metadata.get("content").is_none());
    let second = f
        .store
        .project_document_page(
            &f.ctx,
            &f.project.id,
            &serde_json::from_value(json!({"limit":1,"after":first.next_after})).unwrap(),
        )
        .unwrap();
    assert_eq!(second.items[0].id, ids[1]);
    let doc = f
        .store
        .get_project_document(&f.ctx, &f.project.id, &ids[0])
        .unwrap();
    let version = f
        .store
        .transition_document_version(
            &f.ctx,
            &f.project.id,
            &doc.id,
            &doc.latest_version_id,
            "review",
            &DocumentTransitionRequest {
                approval_id: None,
                expected_state_revision: 1,
                reason: "Approved review".into(),
            },
        )
        .unwrap();
    let published = f
        .store
        .publish_project_document(
            &f.ctx,
            &f.project.id,
            &doc.id,
            &ProjectDocumentPublish {
                expected_revision: doc.revision,
                version_id: version.id.clone(),
                expected_state_revision: version.state_revision,
                reason: "Publish reviewed requirement".into(),
            },
        )
        .unwrap();
    let events = f
        .store
        .document_history_page(&f.ctx, &f.project.id, &doc.id, None, 1)
        .unwrap();
    assert_eq!(events.items[0].event, "document.created");
    assert_eq!(events.items[0].actor_id, f.ctx.actor_id());
    assert!(events.next_after.is_some());
    let next = f
        .store
        .document_history_page(&f.ctx, &f.project.id, &doc.id, events.next_after, 10)
        .unwrap();
    assert_eq!(
        next.items
            .iter()
            .map(|event| event.event.as_str())
            .collect::<Vec<_>>(),
        vec!["document.transitioned", "document.published"]
    );
    let publications = f
        .store
        .document_publication_page(&f.ctx, &f.project.id, &doc.id, None, 1)
        .unwrap();
    assert_eq!(publications.items[0].id, published.id);
    assert_eq!(publications.items[0].checksum, version.checksum);
    let current = f
        .store
        .get_project_document(&f.ctx, &f.project.id, &doc.id)
        .unwrap();
    f.store
        .change_project_document(
            &f.ctx,
            &f.project.id,
            &doc.id,
            &ProjectDocumentChange {
                expected_revision: current.revision,
                archived: true,
                reason: "Keep historical context".into(),
            },
        )
        .unwrap();
    assert_eq!(
        f.store
            .project_document_page(
                &f.ctx,
                &f.project.id,
                &serde_json::from_value(json!({"limit":100})).unwrap()
            )
            .unwrap()
            .items
            .len(),
        2
    );
    assert_eq!(
        f.store
            .project_document_page(
                &f.ctx,
                &f.project.id,
                &serde_json::from_value(json!({"limit":100,"include_archived":true})).unwrap()
            )
            .unwrap()
            .items
            .len(),
        3
    );
    assert!(
        f.store
            .project_document_page(
                &f.ctx,
                &f.project.id,
                &serde_json::from_value(json!({"limit":0})).unwrap()
            )
            .is_err()
    );
    assert!(
        f.store
            .project_document_page(
                &f.ctx,
                &f.project.id,
                &serde_json::from_value(json!({"kind":"task"})).unwrap()
            )
            .is_err()
    );
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Read document catalog",
            KeyScope {
                capabilities: vec![Capability::DocumentsRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let mut reader =
        nexora_platform_store::PlatformStore::connect(&f.config, &f.namespace).unwrap();
    let reader_ctx = reader.authenticate(&key.key).unwrap();
    let query = serde_json::from_value(json!({"include_archived":true,"limit":100})).unwrap();
    assert_eq!(
        reader
            .project_document_page(&reader_ctx, &f.project.id, &query)
            .unwrap()
            .items
            .len(),
        3
    );
    assert!(
        !reader
            .document_history_page(&reader_ctx, &f.project.id, &doc.id, None, 100)
            .unwrap()
            .items
            .is_empty()
    );
    f.store.revoke_key(&f.ctx, &key.metadata.id).unwrap();
    assert!(matches!(
        reader.project_document_page(&reader_ctx, &f.project.id, &query),
        Err(PlatformError::Unauthorized)
    ));
    assert!(matches!(
        reader.document_history_page(&reader_ctx, &f.project.id, &doc.id, None, 100),
        Err(PlatformError::Unauthorized)
    ));
    assert!(matches!(
        reader.document_publication_page(&reader_ctx, &f.project.id, &doc.id, None, 100),
        Err(PlatformError::Unauthorized)
    ));
    assert!(matches!(
        f.store
            .document_history_page(&f.ctx, "foreign-project", &doc.id, None, 10),
        Err(PlatformError::NotFound)
    ));
}
