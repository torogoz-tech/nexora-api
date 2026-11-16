#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::PlatformStore;
use serde_json::{Value, json};
use work_item_fixture::WorkItemFixture;

fn flow(f: &mut WorkItemFixture, human: bool) -> WorkflowVersion {
    let guards = if human {
        json!([{"kind":"reason"},{"kind":"human_approval"}])
    } else {
        json!([{"kind":"reason"}])
    };
    let definition=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Document integrity","purpose":"Explicit publication and rejected states","resource_kinds":["requirement","business_rule","decision","document"],"graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"reviewed","name":"Reviewed","publishable":true},{"id":"rejected","name":"Rejected","terminal":true}],"transitions":[{"id":"review","name":"Review","from":"draft","to":"reviewed","guards":guards},{"id":"reject","name":"Reject","from":"draft","to":"rejected","guards":[{"kind":"reason"}]},{"id":"reopen","name":"Reopen","from":"reviewed","to":"draft","guards":[{"kind":"reason"}]}]}})).unwrap()).unwrap();
    f.store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &definition.id,
            &WorkflowPublish {
                expected_revision: definition.revision,
                reason: "Document review policy".into(),
            },
        )
        .unwrap()
}
fn content() -> Value {
    json!({"kind":"requirement","body":{"problem":"Ambiguous rule","objective":"Declare rule","scope":"Checkout","business_actors":["Customer"],"acceptance_criteria":["Explicit criterion"],"source":"Business interview"}})
}
fn create(
    f: &mut WorkItemFixture,
    flow: &WorkflowVersion,
    code: &str,
    content: Value,
) -> ProjectDocument {
    f.store.create_project_document(&f.ctx,&f.project.id,&serde_json::from_value(json!({"code":code,"title":"Auditable business definition","content":content,"origin":"declared_business","workflow_version_id":flow.id,"reason":"Record structured source"})).unwrap()).unwrap()
}

#[test]
#[ignore = "requires PostgreSQL; exact receipts, stale revisions, immutable content and configured publication guards"]
fn documents_preserve_content_and_refuse_forged_or_stale_publications() {
    let mut f = WorkItemFixture::new();
    assert_eq!(
        f.task("Keep task policy separate").state_id.as_deref(),
        Some("ready")
    );
    let flow = flow(&mut f, false);
    let input:ProjectDocumentCreate=serde_json::from_value(json!({"code":"REQ-replay","title":"Explicit policy","content":content(),"origin":"declared_business","workflow_version_id":flow.id,"reason":"Receipt fixture"})).unwrap();
    let body = json!({"project_id":f.project.id,"definition":input});
    let doc = f
        .store
        .request(&f.ctx, "document.create", "create-once", &body, |s| {
            s.create_project_document(&f.ctx, &f.project.id, &input)
        })
        .unwrap();
    let replay = f
        .store
        .request(&f.ctx, "document.create", "create-once", &body, |s| {
            s.create_project_document(&f.ctx, &f.project.id, &input)
        })
        .unwrap();
    assert_eq!(doc.id, replay.id);
    let version = f
        .store
        .document_version(&f.ctx, &f.project.id, &doc.id, &doc.latest_version_id)
        .unwrap();
    let original_export = f
        .store
        .export_project_document(&f.ctx, &f.project.id, &doc.id, &version.id, "json")
        .unwrap()
        .content;
    let rejected = f
        .store
        .transition_document_version(
            &f.ctx,
            &f.project.id,
            &doc.id,
            &version.id,
            "reject",
            &DocumentTransitionRequest {
                approval_id: None,
                expected_state_revision: 1,
                reason: "Terminal rejection must not imply publication".into(),
            },
        )
        .unwrap();
    let publication = ProjectDocumentPublish {
        expected_revision: doc.revision,
        version_id: version.id.clone(),
        expected_state_revision: rejected.state_revision,
        reason: "Rejected is not publishable".into(),
    };
    assert!(matches!(
        f.store
            .publish_project_document(&f.ctx, &f.project.id, &doc.id, &publication),
        Err(PlatformError::Conflict(_))
    ));
    assert_eq!(
        original_export,
        f.store
            .export_project_document(&f.ctx, &f.project.id, &doc.id, &version.id, "json")
            .unwrap()
            .content
    );
    assert!(
        f.store
            .transition_document_version(
                &f.ctx,
                &f.project.id,
                &doc.id,
                &version.id,
                "review",
                &DocumentTransitionRequest {
                    approval_id: None,
                    expected_state_revision: 1,
                    reason: "Stale state".into()
                }
            )
            .is_err()
    );
    let request:ProjectDocumentRevise=serde_json::from_value(json!({"expected_revision":doc.revision,"title":"New draft","content":content(),"origin":"agent_proposed","workflow_version_id":flow.id,"reason":"New revision"})).unwrap();
    let second = f
        .store
        .revise_project_document(&f.ctx, &f.project.id, &doc.id, &request)
        .unwrap();
    assert!(matches!(
        f.store
            .revise_project_document(&f.ctx, &f.project.id, &doc.id, &request),
        Err(PlatformError::Conflict(_))
    ));
    let current = f
        .store
        .get_project_document(&f.ctx, &f.project.id, &doc.id)
        .unwrap();
    let reviewed = f
        .store
        .transition_document_version(
            &f.ctx,
            &f.project.id,
            &doc.id,
            &second.id,
            "review",
            &DocumentTransitionRequest {
                approval_id: None,
                expected_state_revision: 1,
                reason: "Reviewed".into(),
            },
        )
        .unwrap();
    let publication = ProjectDocumentPublish {
        expected_revision: current.revision,
        version_id: second.id.clone(),
        expected_state_revision: reviewed.state_revision,
        reason: "Publish this exact revision".into(),
    };
    let body = json!({"project_id":f.project.id,"document_id":doc.id,"publication":publication});
    let published = f
        .store
        .request(&f.ctx, "document.publish", "publish-once", &body, |s| {
            s.publish_project_document(&f.ctx, &f.project.id, &doc.id, &publication)
        })
        .unwrap();
    let replay = f
        .store
        .request(&f.ctx, "document.publish", "publish-once", &body, |s| {
            s.publish_project_document(&f.ctx, &f.project.id, &doc.id, &publication)
        })
        .unwrap();
    assert_eq!(published.id, replay.id);
    let current = f
        .store
        .get_project_document(&f.ctx, &f.project.id, &doc.id)
        .unwrap();
    let mut old = publication;
    old.expected_revision = current.revision;
    old.version_id = version.id.clone();
    assert!(
        f.store
            .publish_project_document(&f.ctx, &f.project.id, &doc.id, &old)
            .is_err()
    );
    assert!(
        f.store
            .export_project_document(&f.ctx, &f.project.id, &doc.id, &version.id, "html")
            .is_err()
    );
    assert!(
        f.store
            .document_version_page(&f.ctx, &f.project.id, &doc.id, None, 0)
            .is_err()
    );

    let password =
        std::fs::read_to_string(f.config.secrets_directory.join("postgres_password")).unwrap();
    let mut client = postgres::Config::new()
        .host(&f.config.host)
        .port(f.config.postgres_port)
        .user(&f.config.postgres_user)
        .password(password.trim())
        .dbname(&f.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    client
        .batch_execute(&format!("SET search_path TO \"{}\"", f.namespace))
        .unwrap();
    for statement in [
        "UPDATE project_document_versions SET title='tampered' WHERE id=$1",
        "DELETE FROM project_document_versions WHERE id=$1",
        "UPDATE document_publications SET checksum=repeat('0',64) WHERE version_id=$1",
        "DELETE FROM document_history WHERE version_id=$1",
    ] {
        let error = client.execute(statement, &[&second.id]).unwrap_err();
        assert_eq!(error.code().map(|code| code.code()), Some("23514"));
    }
    let error = client
        .execute(
            "UPDATE document_version_states SET state_id='outside-flow' WHERE version_id=$1",
            &[&second.id],
        )
        .unwrap_err();
    assert_eq!(error.code().map(|code| code.code()), Some("23514"));
    let actual: Vec<String> = client
        .query(
            "SELECT DISTINCT actor_id FROM document_history WHERE document_id=$1",
            &[&doc.id],
        )
        .unwrap()
        .into_iter()
        .map(|r| r.get(0))
        .collect();
    assert_eq!(actual, vec![f.ctx.actor_id().to_owned()]);
    drop(f.store);
    let mut restarted = PlatformStore::connect(&f.config, &f.namespace).unwrap();
    assert_eq!(
        restarted
            .document_version(&f.ctx, &f.project.id, &doc.id, &version.id)
            .unwrap()
            .checksum,
        version.checksum
    );
}

#[test]
#[ignore = "requires PostgreSQL; typed business contents and human guard cannot be forged by author input"]
fn document_kinds_are_structured_and_human_conditions_fail_closed() {
    let mut f = WorkItemFixture::new();
    let guarded = flow(&mut f, true);
    let doc = create(&mut f, &guarded, "human-review", content());
    assert!(
        f.store
            .transition_document_version(
                &f.ctx,
                &f.project.id,
                &doc.id,
                &doc.latest_version_id,
                "review",
                &DocumentTransitionRequest {
                    approval_id: None,
                    expected_state_revision: 1,
                    reason: "Even a human author must supply an exact approval".into()
                }
            )
            .is_err()
    );
    assert!(
        serde_json::from_value::<DocumentTransitionRequest>(
            json!({"expected_state_revision":1,"reason":"Forged","human_approval_valid":true})
        )
        .is_err()
    );
    assert!(serde_json::from_value::<ProjectDocumentCreate>(json!({"code":"forged","title":"Forged author","content":content(),"origin":"declared_business","workflow_version_id":guarded.id,"reason":"Forged","created_by":"someone-else"})).is_err());
    let plain = flow(&mut f, false);
    let contents = [
        json!({"kind":"business_rule","body":{"statement":"Eligible tickets receive discount","conditions":["Ticket is active"],"exceptions":["Cancelled ticket"],"examples":["Active ticket qualifies"],"evidence":["Declared by owner"],"source":"Business policy"}}),
        json!({"kind":"decision","body":{"context":"Multiple repositories","decision":"Pin explicit snapshots","alternatives":["Assume shared commit"],"consequences":["Snapshots are independently identifiable"],"source":"Architecture discussion"}}),
        json!({"kind":"document","body":{"text":"Declared architecture","source":"Developer declaration","architecture":{"style":"Layered","layers":["API","Application","Storage"],"conventions":["One type per file"],"dependency_restrictions":["Domain has no database dependency"]}}}),
    ];
    for (index, content) in contents.into_iter().enumerate() {
        let doc = create(&mut f, &plain, &format!("typed-{index}"), content.clone());
        let actual = f
            .store
            .document_version(&f.ctx, &f.project.id, &doc.id, &doc.latest_version_id)
            .unwrap();
        assert_eq!(actual.content["kind"], content["kind"]);
        assert_eq!(actual.content["body"], content["body"]);
    }
    let mut invalid = content();
    invalid["body"]["acceptance_criteria"] = json!([]);
    let input:ProjectDocumentCreate=serde_json::from_value(json!({"code":"reusable-after-rollback","title":"Invalid acceptance","content":invalid,"origin":"declared_business","workflow_version_id":plain.id,"reason":"Empty criterion"})).unwrap();
    assert!(matches!(
        f.store
            .create_project_document(&f.ctx, &f.project.id, &input),
        Err(PlatformError::Validation(_))
    ));
    create(&mut f, &plain, "reusable-after-rollback", content());
    let mut input = input;
    input.content = serde_json::from_value(content()).unwrap();
    input.code = "unverified-code".into();
    input.origin = DocumentOrigin::ObservedCode;
    assert!(matches!(
        f.store
            .create_project_document(&f.ctx, &f.project.id, &input),
        Err(PlatformError::Validation(_))
    ));
}

#[test]
#[ignore = "requires PostgreSQL; concurrent review and publication each have one winner"]
fn concurrent_document_review_and_publication_have_one_winner() {
    use std::sync::{Arc, Barrier};
    let mut f = WorkItemFixture::new();
    let flow = flow(&mut f, false);
    let doc = create(&mut f, &flow, "concurrent-review", content());
    let config = &f.config;
    let namespace = &f.namespace;
    let ctx = &f.ctx;
    let project = &f.project.id;
    let barrier = Arc::new(Barrier::new(2));
    let outcomes = std::thread::scope(|scope| {
        let handles = (0..2)
            .map(|_| {
                let barrier = barrier.clone();
                let doc = &doc;
                scope.spawn(move || {
                    let mut store = PlatformStore::connect(config, namespace).unwrap();
                    barrier.wait();
                    store.transition_document_version(
                        ctx,
                        project,
                        &doc.id,
                        &doc.latest_version_id,
                        "review",
                        &DocumentTransitionRequest {
                            approval_id: None,
                            expected_state_revision: 1,
                            reason: "Concurrent review".into(),
                        },
                    )
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Err(PlatformError::Conflict(_))))
            .count(),
        1
    );
    let barrier = Arc::new(Barrier::new(2));
    let outcomes = std::thread::scope(|scope| {
        let handles = (0..2)
            .map(|_| {
                let barrier = barrier.clone();
                let doc = &doc;
                scope.spawn(move || {
                    let mut store = PlatformStore::connect(config, namespace).unwrap();
                    barrier.wait();
                    store.publish_project_document(
                        ctx,
                        project,
                        &doc.id,
                        &ProjectDocumentPublish {
                            expected_revision: 1,
                            version_id: doc.latest_version_id.clone(),
                            expected_state_revision: 2,
                            reason: "Concurrent publication".into(),
                        },
                    )
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Err(PlatformError::Conflict(_))))
            .count(),
        1
    );
    assert_eq!(
        f.store
            .get_project_document(ctx, project, &doc.id)
            .unwrap()
            .revision,
        2
    );
}
