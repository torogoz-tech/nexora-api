#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; explicit version links govern task document prerequisites and deterministic context packages"]
fn version_traceability_and_context_packages_keep_business_work_auditable() {
    let mut f = WorkItemFixture::new();
    f.task("Existing tasks remain independent of document inference");
    let workflow=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Business review","purpose":"Explicit declared business approval","resource_kinds":["requirement","business_rule"],"graph":{"states":[{"id":"draft","name":"Draft","initial":true},{"id":"reviewed","name":"Reviewed","publishable":true}],"transitions":[{"id":"review","name":"Review","from":"draft","to":"reviewed","guards":[{"kind":"reason"}]}]}})).unwrap()).unwrap();
    let flow = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &workflow.id,
            &WorkflowPublish {
                expected_revision: workflow.revision,
                reason: "Publish review policy".into(),
            },
        )
        .unwrap();
    let task_workflow=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Requirement-backed delivery","purpose":"Configured document prerequisite","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"reason"},{"kind":"approved_document_version"}]},{"id":"reopen","name":"Reopen","from":"done","to":"ready","guards":[{"kind":"reason"}]}]}})).unwrap()).unwrap();
    let task_flow = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &task_workflow.id,
            &WorkflowPublish {
                expected_revision: task_workflow.revision,
                reason: "Approved prerequisite is an actual server fact".into(),
            },
        )
        .unwrap();
    let mut task=f.store.create_task(&f.ctx,&f.project.id,&serde_json::from_value(json!({"title":"Implement declared eligibility","type_code":"task","workflow_version_id":task_flow.id,"origin":"External developer","fields":{"approved_document_version":true}})).unwrap()).unwrap();
    assert!(f.store.transition_task(&f.ctx,&f.project.id,&task.id,"finish",&serde_json::from_value(json!({"expected_revision":task.revision,"reason":"Custom fields cannot forge approval"})).unwrap()).is_err());
    let content = json!({"kind":"requirement","body":{"problem":"Unclear eligibility","objective":"Make conditions explicit","scope":"Checkout","business_actors":["Customer"],"acceptance_criteria":["Condition is testable"],"source":"Business owner"}});
    let doc=f.store.create_project_document(&f.ctx,&f.project.id,&serde_json::from_value(json!({"code":"REQ-TRACE","title":"Eligibility","content":content,"origin":"declared_business","workflow_version_id":flow.id,"reason":"Record explicit rule"})).unwrap()).unwrap();
    let link_input=serde_json::from_value(json!({"expected_document_revision":doc.revision,"version_id":doc.latest_version_id,"target":{"kind":"task","task_id":task.id,"expected_revision":task.revision},"kind":"required_for_transition","reason":"Task requires this exact approved version"})).unwrap();
    let body = json!({"project_id":f.project.id,"document_id":doc.id,"link":link_input});
    let link = f
        .store
        .request(
            &f.ctx,
            "document.link_create",
            "link-once",
            &body,
            |store| store.create_document_link(&f.ctx, &f.project.id, &doc.id, &link_input),
        )
        .unwrap();
    let replay = f
        .store
        .request(
            &f.ctx,
            "document.link_create",
            "link-once",
            &body,
            |store| store.create_document_link(&f.ctx, &f.project.id, &doc.id, &link_input),
        )
        .unwrap();
    assert_eq!(link.id, replay.id);
    task = f.store.get_task(&f.ctx, &f.project.id, &task.id).unwrap();
    assert_eq!(task.revision, 2);
    assert!(f.store.transition_task(&f.ctx,&f.project.id,&task.id,"finish",&serde_json::from_value(json!({"expected_revision":task.revision,"reason":"Linked draft is not approved"})).unwrap()).is_err());
    let reviewed = f
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
                reason: "Business review".into(),
            },
        )
        .unwrap();
    let current = f
        .store
        .get_project_document(&f.ctx, &f.project.id, &doc.id)
        .unwrap();
    f.store
        .publish_project_document(
            &f.ctx,
            &f.project.id,
            &doc.id,
            &ProjectDocumentPublish {
                expected_revision: current.revision,
                version_id: reviewed.id.clone(),
                expected_state_revision: reviewed.state_revision,
                reason: "Publish reviewed requirement".into(),
            },
        )
        .unwrap();
    task = f
        .store
        .transition_task(
            &f.ctx,
            &f.project.id,
            &task.id,
            "finish",
            &serde_json::from_value(
                json!({"expected_revision":task.revision,"reason":"Pinned version is approved"}),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(task.state_id.as_deref(), Some("done"));
    task = f
        .store
        .transition_task(
            &f.ctx,
            &f.project.id,
            &task.id,
            "reopen",
            &serde_json::from_value(
                json!({"expected_revision":task.revision,"reason":"Next requirement revision"}),
            )
            .unwrap(),
        )
        .unwrap();
    let current = f
        .store
        .get_project_document(&f.ctx, &f.project.id, &doc.id)
        .unwrap();
    let next=f.store.revise_project_document(&f.ctx,&f.project.id,&doc.id,&serde_json::from_value(json!({"expected_revision":current.revision,"title":"Eligibility clarified","content":content,"origin":"agent_proposed","workflow_version_id":flow.id,"reason":"Proposed revision keeps old publication valid"})).unwrap()).unwrap();
    task=f.store.transition_task(&f.ctx,&f.project.id,&task.id,"finish",&serde_json::from_value(json!({"expected_revision":task.revision,"reason":"Old approved version remains selected while a new draft exists"})).unwrap()).unwrap();
    task=f.store.transition_task(&f.ctx,&f.project.id,&task.id,"reopen",&serde_json::from_value(json!({"expected_revision":task.revision,"reason":"Use the next approved version explicitly"})).unwrap()).unwrap();
    let reviewed = f
        .store
        .transition_document_version(
            &f.ctx,
            &f.project.id,
            &doc.id,
            &next.id,
            "review",
            &DocumentTransitionRequest {
                approval_id: None,
                expected_state_revision: 1,
                reason: "Review new version".into(),
            },
        )
        .unwrap();
    let current = f
        .store
        .get_project_document(&f.ctx, &f.project.id, &doc.id)
        .unwrap();
    f.store
        .publish_project_document(
            &f.ctx,
            &f.project.id,
            &doc.id,
            &ProjectDocumentPublish {
                expected_revision: current.revision,
                version_id: reviewed.id.clone(),
                expected_state_revision: reviewed.state_revision,
                reason: "Supersede approved version".into(),
            },
        )
        .unwrap();
    assert!(f.store.transition_task(&f.ctx,&f.project.id,&task.id,"finish",&serde_json::from_value(json!({"expected_revision":task.revision,"reason":"Old link must not float to the new publication"})).unwrap()).is_err());
    let current = f
        .store
        .get_project_document(&f.ctx, &f.project.id, &doc.id)
        .unwrap();
    f.store.remove_document_link(&f.ctx,&f.project.id,&doc.id,&link.id,&serde_json::from_value(json!({"expected_document_revision":current.revision,"expected_target_revision":task.revision,"reason":"Retain history; select new prerequisite"})).unwrap()).unwrap();
    task = f.store.get_task(&f.ctx, &f.project.id, &task.id).unwrap();
    let current = f
        .store
        .get_project_document(&f.ctx, &f.project.id, &doc.id)
        .unwrap();
    let new_link=f.store.create_document_link(&f.ctx,&f.project.id,&doc.id,&serde_json::from_value(json!({"expected_document_revision":current.revision,"version_id":next.id,"target":{"kind":"task","task_id":task.id,"expected_revision":task.revision},"kind":"required_for_transition","reason":"Explicit prerequisite version update"})).unwrap()).unwrap();
    let links = f
        .store
        .task_document_links_page(&f.ctx, &f.project.id, &task.id, None, 10, false)
        .unwrap();
    assert_eq!(links.items.len(), 1);
    assert_eq!(links.items[0].id, new_link.id);
    let all = f
        .store
        .document_links_page(&f.ctx, &f.project.id, &doc.id, None, 10, true)
        .unwrap();
    assert_eq!(all.items.len(), 2);
    task = f.store.get_task(&f.ctx, &f.project.id, &task.id).unwrap();
    task=f.store.transition_task(&f.ctx,&f.project.id,&task.id,"finish",&serde_json::from_value(json!({"expected_revision":task.revision,"reason":"New approved prerequisite is satisfied"})).unwrap()).unwrap();
    let rule=f.store.create_project_document(&f.ctx,&f.project.id,&serde_json::from_value(json!({"code":"RULE-TRACE","title":"Eligibility rule","content":{"kind":"business_rule","body":{"statement":"Eligible customers receive the declared discount","conditions":["Customer meets documented eligibility"],"source":"Business owner"}},"origin":"declared_business","workflow_version_id":flow.id,"reason":"Separate rule linked to its requirement"})).unwrap()).unwrap();
    let current = f
        .store
        .get_project_document(&f.ctx, &f.project.id, &doc.id)
        .unwrap();
    let rule_link=f.store.create_document_link(&f.ctx,&f.project.id,&rule.id,&serde_json::from_value(json!({"expected_document_revision":rule.revision,"version_id":rule.latest_version_id,"target":{"kind":"document_version","document_id":doc.id,"version_id":next.id,"expected_revision":current.revision},"kind":"implements","reason":"Record exact rule-to-requirement traceability"})).unwrap()).unwrap();
    let rule_links = f
        .store
        .document_links_page(&f.ctx, &f.project.id, &rule.id, None, 10, false)
        .unwrap();
    assert_eq!(rule_links.items[0].id, rule_link.id);
    let package=f.store.create_project_context_package(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Implementation context","document_versions":[{"document_id":doc.id,"version_id":next.id},{"document_id":rule.id,"version_id":rule.latest_version_id}],"task_ids":[task.id],"reason":"Prepare bounded deterministic context for external agents"})).unwrap()).unwrap();
    let export = f
        .store
        .export_project_context_package(&f.ctx, &f.project.id, &package.id, "json")
        .unwrap();
    assert!(export.content.contains("Make conditions explicit"));
    assert!(
        export
            .content
            .contains("Eligible customers receive the declared discount")
    );
    assert!(export.content.contains("Implement declared eligibility"));
    assert!(!package.needs_review);
    f.store.change_task(&f.ctx,&f.project.id,&task.id,&serde_json::from_value(json!({"expected_revision":task.revision,"title":"Implementation context changed","reason":"Captured package must remain historical"})).unwrap()).unwrap();
    let historical = f
        .store
        .get_project_context_package(&f.ctx, &f.project.id, &package.id)
        .unwrap();
    assert_eq!(historical.checksum, package.checksum);
    assert!(historical.needs_review);
    assert_eq!(
        export.content,
        f.store
            .export_project_context_package(&f.ctx, &f.project.id, &package.id, "json")
            .unwrap()
            .content
    );
    let history = f
        .store
        .task_history(&f.ctx, &f.project.id, &task.id, None, 100)
        .unwrap();
    assert!(
        history
            .items
            .iter()
            .any(|event| event.operation == "task.document_linked")
    );
    assert!(matches!(
        f.store
            .get_project_context_package(&f.ctx, "foreign-project", &package.id),
        Err(PlatformError::NotFound)
    ));
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Document-only package reader",
            KeyScope {
                capabilities: vec![Capability::DocumentsRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let mut restarted =
        nexora_platform_store::PlatformStore::connect(&f.config, &f.namespace).unwrap();
    assert_eq!(
        restarted
            .get_project_context_package(&f.ctx, &f.project.id, &package.id)
            .unwrap()
            .checksum,
        package.checksum
    );
    let limited = restarted.authenticate(&key.key).unwrap();
    assert!(matches!(
        restarted.get_project_context_package(&limited, &f.project.id, &package.id),
        Err(PlatformError::NotFound)
    ));
    f.store.revoke_key(&f.ctx, &key.metadata.id).unwrap();
    assert!(matches!(
        restarted.get_project_context_package(&limited, &f.project.id, &package.id),
        Err(PlatformError::Unauthorized)
    ));
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(f.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&f.config.host)
        .port(f.config.postgres_port)
        .user(&f.config.postgres_user)
        .password(password.trim())
        .dbname(&f.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{}\"", f.namespace))
        .unwrap();
    for statement in [
        "UPDATE project_context_packages SET name='tampered' WHERE id=$1",
        "DELETE FROM project_context_packages WHERE id=$1",
    ] {
        let error = sql.execute(statement, &[&package.id]).unwrap_err();
        assert_eq!(error.code().map(|code| code.code()), Some("23514"));
    }
    for statement in [
        "UPDATE document_links SET kind='references' WHERE id=$1",
        "DELETE FROM document_links WHERE id=$1",
    ] {
        let error = sql.execute(statement, &[&new_link.id]).unwrap_err();
        assert_eq!(error.code().map(|code| code.code()), Some("23514"));
    }
    let invalid = nexora_platform_store::SecretGenerator::id("invalid_link").unwrap();
    let error=sql.execute("INSERT INTO document_links(id,project_id,source_document_id,source_version_id,kind,target_task_id,target_document_id,target_version_id,source_revision_at_creation,target_revision_at_creation,reason,created_by,credential_id,removed_at,removed_by,removal_reason) SELECT $1,project_id,source_document_id,source_version_id,kind,target_task_id,target_document_id,target_version_id,source_revision_at_creation,target_revision_at_creation,reason,created_by,credential_id,clock_timestamp(),created_by,NULL FROM document_links WHERE id=$2",&[&invalid,&new_link.id]).unwrap_err();
    assert_eq!(error.code().map(|code| code.code()), Some("23514"));
}
