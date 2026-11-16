use nexora_platform::{
    ActorKind, BacklogChange, BacklogCreate, BacklogReorder, Capability, KeyScope, PlatformError,
    TaskChange, TaskCreate, TaskListQuery, TaskPriority, TaskTransitionRequest, TaskTypeCreate,
    WorkflowChange, WorkflowCreate, WorkflowGraph, WorkflowGuard, WorkflowPublish,
    WorkflowResourceKind,
};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::json;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires PostgreSQL; project backlog and tasks pin workflow versions and permit agents according to guards, never roles"]
fn persistent_work_items_follow_pinned_flows_with_scoped_identity_and_history() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_work_items_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    store
        .bootstrap("Work item fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let project = store.create_project(&ctx, "Project work", None).unwrap();
    let graph:WorkflowGraph=serde_json::from_value(json!({
        "states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],
        "transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[]}]
    })).unwrap();
    let workflow = store
        .create_workflow(
            &ctx,
            &project.id,
            &WorkflowCreate {
                icon: nexora_platform::WorkflowIcon::default(),
                color: None,
                name: "Delivery".into(),
                purpose: "Explicit task flow".into(),
                resource_kinds: vec![WorkflowResourceKind::Task],
                graph: graph.clone(),
            },
        )
        .unwrap();
    let version = store
        .publish_workflow(
            &ctx,
            &project.id,
            &workflow.id,
            &WorkflowPublish {
                expected_revision: workflow.revision,
                reason: "Initial task flow".into(),
            },
        )
        .unwrap();
    let kind = store
        .create_task_type(
            &ctx,
            &project.id,
            &TaskTypeCreate {
                purpose: nexora_platform::TaskPurpose::General,
                code: "bug".into(),
                name: "Bug".into(),
                description: None,
                color: None,
            },
        )
        .unwrap();
    let backlog = store
        .create_backlog(
            &ctx,
            &project.id,
            &BacklogCreate {
                name: "Delivery".into(),
                description: Some("Project work".into()),
                workflow_version_id: version.id.clone(),
                color: None,
            },
        )
        .unwrap();
    let actor = store
        .create_actor(
            &ctx,
            ActorKind::Agent,
            "Developer",
            "External developer work",
        )
        .unwrap();
    let capabilities = vec![
        Capability::TasksRead,
        Capability::TasksWrite,
        Capability::TasksTransition,
    ];
    for capability in &capabilities {
        store
            .grant(&ctx, &actor.id, *capability, Some(&project.id))
            .unwrap();
    }
    let key = store
        .issue_key(
            &ctx,
            &actor.id,
            "Task actor",
            KeyScope {
                capabilities,
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let agent = store.authenticate(&key.key).unwrap();
    let create = TaskCreate {
        title: "First bug".into(),
        context: "Reproduce and fix".into(),
        type_code: kind.code.clone(),
        backlog_id: Some(backlog.id.clone()),
        origin: "Fixture contribution".into(),
        ..Default::default()
    };
    let first = store.create_task(&agent, &project.id, &create).unwrap();
    assert_eq!(first.state_id.as_deref(), Some("ready"));
    assert_eq!(
        first.workflow_version_id.as_deref(),
        Some(version.id.as_str())
    );
    assert_eq!(first.created_by, actor.id);
    assert!(matches!(
        store.change_task(
            &agent,
            &project.id,
            &first.id,
            &TaskChange {
                expected_revision: first.revision,
                assignee_id: Some(actor.id.clone()),
                reason: "Assignee without a published developer workflow".into(),
                ..Default::default()
            },
        ),
        Err(PlatformError::Validation(_))
    ));
    let changed = store
        .change_task(
            &agent,
            &project.id,
            &first.id,
            &TaskChange {
                expected_revision: first.revision,
                priority: Some(TaskPriority::High),
                reason: "Prioritize investigation".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let completed = store
        .transition_task(
            &agent,
            &project.id,
            &first.id,
            "finish",
            &TaskTransitionRequest {
                expected_revision: changed.revision,
                resolution: Some("Fixed and verified".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        completed.state_id.as_deref(),
        Some("done"),
        "An authorized agent may close when the flow allows it"
    );
    assert!(matches!(
        store.transition_task(
            &agent,
            &project.id,
            &first.id,
            "finish",
            &TaskTransitionRequest {
                expected_revision: changed.revision,
                ..Default::default()
            }
        ),
        Err(PlatformError::Conflict(_))
    ));
    let mut second_input = create.clone();
    second_input.title = "Second bug".into();
    let second = store
        .create_task(&agent, &project.id, &second_input)
        .unwrap();
    let current = store.get_backlog(&ctx, &project.id, &backlog.id).unwrap();
    store
        .reorder_backlog(
            &ctx,
            &project.id,
            &backlog.id,
            &BacklogReorder {
                expected_revision: current.revision,
                task_id: second.id.clone(),
                before_id: Some(first.id.clone()),
                after_id: None,
            },
        )
        .unwrap();
    let page = store
        .task_page(
            &agent,
            &project.id,
            &TaskListQuery {
                backlog_id: Some(backlog.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(page.items[0].id, second.id);
    let history = store
        .task_history(&agent, &project.id, &first.id, None, 50)
        .unwrap();
    assert!(
        history
            .items
            .iter()
            .any(|event| event.operation == "task.transitioned" && event.actor_id == actor.id)
    );
    let current = store.get_workflow(&ctx, &project.id, &workflow.id).unwrap();
    let mut guarded = graph;
    guarded.transitions[0]
        .guards
        .push(WorkflowGuard::HumanApproval);
    let changed = store
        .change_workflow(
            &ctx,
            &project.id,
            &workflow.id,
            &WorkflowChange {
                expected_revision: current.revision,
                graph: Some(guarded),
                reason: "Require explicit human authorization".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let next = store
        .publish_workflow(
            &ctx,
            &project.id,
            &workflow.id,
            &WorkflowPublish {
                expected_revision: changed.revision,
                reason: "Publish approval policy".into(),
            },
        )
        .unwrap();
    let current = store.get_backlog(&ctx, &project.id, &backlog.id).unwrap();
    store
        .change_backlog(
            &ctx,
            &project.id,
            &backlog.id,
            &BacklogChange {
                expected_revision: current.revision,
                workflow_version_id: Some(next.id.clone()),
                reason: "Default for new work only".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        store
            .get_task(&agent, &project.id, &first.id)
            .unwrap()
            .workflow_version_id,
        Some(version.id)
    );
    let guarded_task = store.create_task(&agent, &project.id, &create).unwrap();
    assert_eq!(guarded_task.workflow_version_id, Some(next.id));
    assert!(
        store
            .transition_task(
                &agent,
                &project.id,
                &guarded_task.id,
                "finish",
                &TaskTransitionRequest {
                    expected_revision: guarded_task.revision,
                    ..Default::default()
                }
            )
            .is_err(),
        "A declared human approval guard must not be bypassed"
    );
    assert!(matches!(
        store.get_task(&agent, "foreign-project", &first.id),
        Err(PlatformError::NotFound)
    ));
    drop(store);
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    assert_eq!(
        store
            .get_task(&agent, &project.id, &first.id)
            .unwrap()
            .state_id,
        Some("done".into())
    );
    store.revoke_key(&ctx, &key.metadata.id).unwrap();
    assert!(matches!(
        store.get_task(&agent, &project.id, &first.id),
        Err(PlatformError::Unauthorized)
    ));
}
