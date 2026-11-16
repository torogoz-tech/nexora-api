#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; explicit workflow migration needs complete maps and governance capabilities, invalidates old approvals"]
fn workflow_migration_is_explicit_audited_and_invalidates_previous_authorizations() {
    let mut f = WorkItemFixture::new();
    f.task("Ordinary fixture remains separate");
    let old=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Original review","purpose":"Pinned human authorization","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"human_approval"},{"kind":"reason"}]}]}})).unwrap()).unwrap();
    let old = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &old.id,
            &WorkflowPublish {
                expected_revision: old.revision,
                reason: "Original policy".into(),
            },
        )
        .unwrap();
    let task=f.store.create_task(&f.ctx,&f.project.id,&serde_json::from_value(json!({"title":"Explicit policy migration","type_code":"task","workflow_version_id":old.id,"origin":"Migration fixture","fields":{"business_reference":"keep"}})).unwrap()).unwrap();
    let operation = TaskTransitionRequest {
        expected_revision: task.revision,
        reason: Some("Execute reviewed operation".into()),
        ..Default::default()
    };
    let old_approval = f
        .store
        .grant_task_approval(
            &f.ctx,
            &f.project.id,
            &task.id,
            &TaskApprovalGrant {
                transition_id: "finish".into(),
                parameters: operation.clone(),
                reason: "Human review before policy changes".into(),
                expires_in_seconds: 3600,
            },
        )
        .unwrap();
    let next=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"New review policy","purpose":"New explicit state vocabulary","resource_kinds":["task"],"graph":{"states":[{"id":"queued","name":"Queued","initial":true},{"id":"closed","name":"Closed","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"queued","to":"closed","guards":[{"kind":"human_approval"},{"kind":"reason"}]}]}})).unwrap()).unwrap();
    let next = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &next.id,
            &WorkflowPublish {
                expected_revision: next.revision,
                reason: "Publish target policy without implicit migration".into(),
            },
        )
        .unwrap();
    assert_eq!(
        f.store
            .get_task(&f.ctx, &f.project.id, &task.id)
            .unwrap()
            .workflow_version_id,
        Some(old.id)
    );
    let incomplete = json!({"expected_revision":task.revision,"workflow_version_id":next.id,"state_mapping":{"ready":"queued"},"reason":"Incomplete mapping"});
    assert!(matches!(
        f.store.migrate_task_workflow(
            &f.ctx,
            &f.project.id,
            &task.id,
            &serde_json::from_value(incomplete).unwrap()
        ),
        Err(PlatformError::Validation(_))
    ));
    let invalid = json!({"expected_revision":task.revision,"workflow_version_id":next.id,"state_mapping":{"ready":"missing","done":"closed"},"reason":"Unknown destination state"});
    assert!(matches!(
        f.store.migrate_task_workflow(
            &f.ctx,
            &f.project.id,
            &task.id,
            &serde_json::from_value(invalid).unwrap()
        ),
        Err(PlatformError::Validation(_))
    ));
    let migration=serde_json::from_value(json!({"expected_revision":task.revision,"workflow_version_id":next.id,"state_mapping":{"ready":"queued","done":"closed"},"reason":"Explicitly adopt target policy"})).unwrap();
    let limited_key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Developer without policy governance",
            KeyScope {
                capabilities: vec![Capability::TasksWrite, Capability::TasksTransition],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let limited = f.store.authenticate(&limited_key.key).unwrap();
    assert!(matches!(
        f.store
            .migrate_task_workflow(&limited, &f.project.id, &task.id, &migration),
        Err(PlatformError::Forbidden)
    ));
    let governor = f
        .store
        .create_actor(
            &f.ctx,
            ActorKind::Agent,
            "Authorized policy migrator",
            "Explicit project governance, no role",
        )
        .unwrap();
    let capabilities = vec![
        Capability::TasksRead,
        Capability::TasksWrite,
        Capability::TasksTransition,
        Capability::WorkflowsPublish,
    ];
    for capability in &capabilities {
        f.store
            .grant(&f.ctx, &governor.id, *capability, Some(&f.project.id))
            .unwrap();
    }
    let key = f
        .store
        .issue_key(
            &f.ctx,
            &governor.id,
            "Policy migration client",
            KeyScope {
                capabilities,
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let governor = f.store.authenticate(&key.key).unwrap();
    let canonical = json!({"project_id":f.project.id,"task_id":task.id,"migration":migration});
    let migrated = f
        .store
        .request(
            &governor,
            "task.workflow_migrate",
            "migrate-once",
            &canonical,
            |store| store.migrate_task_workflow(&governor, &f.project.id, &task.id, &migration),
        )
        .unwrap();
    assert_eq!(migrated.workflow_version_id, Some(next.id));
    assert_eq!(migrated.workflow_id, Some(next.workflow_id));
    assert_eq!(migrated.state_id.as_deref(), Some("queued"));
    assert_eq!(migrated.revision, task.revision + 1);
    assert_eq!(migrated.backlog_id, task.backlog_id);
    assert_eq!(migrated.fields, task.fields);
    assert_eq!(migrated.origin, task.origin);
    let mut stale = operation.clone();
    stale.expected_revision = migrated.revision;
    stale.approval_id = Some(old_approval.id.clone());
    assert!(
        f.store
            .transition_task(&f.ctx, &f.project.id, &task.id, "finish", &stale)
            .is_err()
    );
    assert!(
        f.store
            .get_task_approval(&f.ctx, &f.project.id, &task.id, &old_approval.id)
            .unwrap()
            .consumed_at
            .is_none()
    );
    let new_operation = TaskTransitionRequest {
        expected_revision: migrated.revision,
        reason: operation.reason,
        ..Default::default()
    };
    let authorization = f
        .store
        .grant_task_approval(
            &f.ctx,
            &f.project.id,
            &task.id,
            &TaskApprovalGrant {
                transition_id: "finish".into(),
                parameters: new_operation.clone(),
                reason: "Human review under target policy".into(),
                expires_in_seconds: 600,
            },
        )
        .unwrap();
    let mut execute = new_operation;
    execute.approval_id = Some(authorization.id);
    assert!(
        matches!(
            f.store
                .transition_task(&governor, &f.project.id, &task.id, "finish", &execute),
            Err(PlatformError::NotFound)
        ),
        "policy governance does not grant access to execute unrelated tasks"
    );
    let done = f
        .store
        .transition_task(&f.ctx, &f.project.id, &task.id, "finish", &execute)
        .unwrap();
    assert_eq!(done.state_id.as_deref(), Some("closed"));
    assert_eq!(
        f.store
            .request(
                &governor,
                "task.workflow_migrate",
                "migrate-once",
                &canonical,
                |store| store.migrate_task_workflow(&governor, &f.project.id, &task.id, &migration)
            )
            .unwrap()
            .state_id,
        Some("queued".into())
    );
    let history = f
        .store
        .task_history(&f.ctx, &f.project.id, &task.id, None, 100)
        .unwrap();
    assert_eq!(
        history
            .items
            .iter()
            .filter(|entry| entry.operation == "task.workflow_migrated")
            .count(),
        1
    );
    drop(f.store);
    let mut restarted =
        nexora_platform_store::PlatformStore::connect(&f.config, &f.namespace).unwrap();
    assert_eq!(
        restarted
            .get_task(&f.ctx, &f.project.id, &task.id)
            .unwrap()
            .state_id,
        Some("closed".into())
    );
}
