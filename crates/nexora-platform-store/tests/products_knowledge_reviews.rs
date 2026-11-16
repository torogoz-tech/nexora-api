#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL"]
fn products_group_projects_and_hide_technical_links_in_request_catalog() {
    let mut f = WorkItemFixture::new();
    let product = f
        .store
        .save_product(
            &f.ctx,
            &json!({"name":"Smart Ticket","project_ids":[f.project.id]}),
        )
        .unwrap();
    let catalog = f.store.products(&f.ctx, true).unwrap();
    assert_eq!(catalog["items"][0]["project_ids"][0], f.project.id);
    let public = f.store.products(&f.ctx, false).unwrap();
    assert!(public["items"][0].get("project_ids").is_none());
    let request = f.store.mutate_service_request(&f.ctx,None,"create",&json!({"title":"Cannot sign in","type_id":"support","body":"OTP unavailable","product_id":product["id"]})).unwrap();
    assert_eq!(request["product_id"], product["id"]);
    assert!(matches!(f.store.save_product(&f.ctx,&json!({"id":product["id"],"name":"Changed","archived":false,"expected_revision":0,"project_ids":[]})),Err(PlatformError::Conflict(_))));
}

#[test]
#[ignore = "requires PostgreSQL"]
fn organizational_knowledge_review_pins_revision_and_publishes_without_workflow() {
    let mut f = WorkItemFixture::new();
    let write = |revision| OrganizationDocumentWrite {
        kind: "requirement".into(),
        title: "OTP".into(),
        body: "Verify access".into(),
        expected_revision: revision,
    };
    let doc = f
        .store
        .organization_knowledge_write(&f.ctx, None, &write(None))
        .unwrap();
    let id = doc["id"].as_str().unwrap();
    let review = f
        .store
        .request_knowledge_review(
            &f.ctx,
            &json!({"kind":"organization_document","resource_id":id,"expected_revision":1}),
        )
        .unwrap();
    f.store
        .organization_knowledge_write(&f.ctx, Some(id), &write(Some(1)))
        .unwrap();
    assert!(matches!(
        f.store.decide_knowledge_review(
            &f.ctx,
            review["id"].as_str().unwrap(),
            &json!({"expected_revision":1,"decision":"approved"})
        ),
        Err(PlatformError::Conflict(_))
    ));
    let current = f
        .store
        .request_knowledge_review(
            &f.ctx,
            &json!({"kind":"organization_document","resource_id":id,"expected_revision":2}),
        )
        .unwrap();
    let same = f
        .store
        .request_knowledge_review(
            &f.ctx,
            &json!({"kind":"organization_document","resource_id":id,"expected_revision":2}),
        )
        .unwrap();
    assert_eq!(current["id"], same["id"]);
    f.store
        .decide_knowledge_review(
            &f.ctx,
            current["id"].as_str().unwrap(),
            &json!({"expected_revision":1,"decision":"approved"}),
        )
        .unwrap();
    assert_eq!(
        f.store
            .organization_knowledge_get(&f.ctx, id, None)
            .unwrap()["published_revision"],
        2
    );
}

#[test]
#[ignore = "requires PostgreSQL"]
fn project_requirement_review_publishes_exact_version_and_lists_pending_queue() {
    let mut f = WorkItemFixture::new();
    let doc=f.store.create_project_document(&f.ctx,&f.project.id,&serde_json::from_value(json!({"code":"REQ-OTP","title":"OTP access","content":{"kind":"requirement","body":{"problem":"Access is unverified","objective":"Verify access","scope":"Sign in","business_actors":["User"],"acceptance_criteria":["OTP expires after five minutes"],"source":"Business owner"}},"origin":"declared_business","reason":"Record requirement"})).unwrap()).unwrap();
    assert!(
        f.store
            .document_version(&f.ctx, &f.project.id, &doc.id, &doc.latest_version_id)
            .unwrap()
            .workflow_version_id
            .is_none()
    );
    let input =
        json!({"kind":"project_document","resource_id":doc.id,"expected_revision":doc.revision});
    let review = f.store.request_knowledge_review(&f.ctx, &input).unwrap();
    let queue = f
        .store
        .knowledge_reviews(&f.ctx, None, "pending", 1)
        .unwrap();
    assert_eq!(queue["total"], 1);
    assert_eq!(queue["items"][0]["can_decide"], true);
    f.store
        .decide_knowledge_review(
            &f.ctx,
            review["id"].as_str().unwrap(),
            &json!({"decision":"approved","expected_revision":1}),
        )
        .unwrap();
    let published = f
        .store
        .get_project_document(&f.ctx, &f.project.id, &doc.id)
        .unwrap();
    assert_eq!(published.published_version_id, Some(doc.latest_version_id));
    assert_eq!(
        f.store
            .knowledge_reviews(&f.ctx, None, "pending", 1)
            .unwrap()["total"],
        0
    );
}

#[test]
#[ignore = "requires PostgreSQL"]
fn mcp_can_request_and_read_reviews_but_cannot_approve_them() {
    let mut f = WorkItemFixture::new();
    let key = f
        .store
        .issue_personal_mcp_key(
            &f.ctx,
            &PersonalMcpKeyCreate {
                name: "Knowledge assistant".into(),
                project_ids: vec![],
                read_only: false,
                include_global: true,
                expires_in_seconds: 86400,
            },
        )
        .unwrap();
    let agent = f.store.authenticate(&key.key).unwrap();
    let doc = f
        .store
        .organization_knowledge_write(
            &f.ctx,
            None,
            &OrganizationDocumentWrite {
                kind: "security".into(),
                title: "Secret handling".into(),
                body: "Do not publish secrets".into(),
                expected_revision: None,
            },
        )
        .unwrap();
    let review = f
        .store
        .request_knowledge_review(
            &agent,
            &json!({"kind":"organization_document","resource_id":doc["id"],"expected_revision":1}),
        )
        .unwrap();
    assert_eq!(
        f.store
            .knowledge_reviews(&agent, None, "pending", 1)
            .unwrap()["items"][0]["can_decide"],
        false
    );
    assert!(matches!(
        f.store.decide_knowledge_review(
            &agent,
            review["id"].as_str().unwrap(),
            &json!({"decision":"approved","expected_revision":1})
        ),
        Err(PlatformError::Forbidden)
    ));
}
