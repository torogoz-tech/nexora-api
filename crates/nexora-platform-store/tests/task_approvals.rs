use nexora_platform::{
    ActorKind, BacklogCreate, Capability, KeyScope, TaskApprovalGrant, TaskChange, TaskCreate,
    TaskPriority, TaskTransitionRequest, TaskTypeCreate, WorkflowCreate, WorkflowGraph,
    WorkflowPublish, WorkflowResourceKind,
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
#[ignore = "requires PostgreSQL; human authorization binds exact task revision and transition, consumed atomically by external agent"]
fn human_task_approval_is_exact_and_cannot_be_forged_or_reused() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_task_approvals_{}",
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
        "transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"human_approval"}]}]
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
        Capability::ApprovalsGrant,
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
    let task = store.create_task(&agent, &project.id, &create).unwrap();
    let grant = TaskApprovalGrant {
        parameters: TaskTransitionRequest {
            expected_revision: task.revision,
            ..Default::default()
        },
        transition_id: "finish".into(),
        reason: "Reviewed exact change".into(),
        expires_in_seconds: 3600,
    };
    assert!(
        store
            .grant_task_approval(&agent, &project.id, &task.id, &grant)
            .is_err(),
        "an agent with approvals capability still cannot provide human authorization"
    );
    let technical_key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Technical key is not approval login",
            KeyScope {
                capabilities: vec![Capability::ApprovalsGrant],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let technical = store.authenticate(&technical_key.key).unwrap();
    assert!(
        store
            .grant_task_approval(&technical, &project.id, &task.id, &grant)
            .is_err(),
        "even a human-owned API key does not satisfy the interactive-session approval policy"
    );
    assert!(
        store
            .grant_task_approval(
                &ctx,
                &project.id,
                &task.id,
                &TaskApprovalGrant {
                    expires_in_seconds: 0,
                    ..grant.clone()
                }
            )
            .is_err()
    );
    let old = store
        .grant_task_approval(&ctx, &project.id, &task.id, &grant)
        .unwrap();
    let changed = store
        .change_task(
            &agent,
            &project.id,
            &task.id,
            &TaskChange {
                expected_revision: task.revision,
                priority: Some(TaskPriority::High),
                reason: "Change invalidates earlier approval".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        store
            .transition_task(
                &agent,
                &project.id,
                &task.id,
                "finish",
                &TaskTransitionRequest {
                    expected_revision: changed.revision,
                    approval_id: Some(old.id.clone()),
                    ..Default::default()
                }
            )
            .is_err()
    );
    let grant = TaskApprovalGrant {
        parameters: TaskTransitionRequest {
            expected_revision: changed.revision,
            ..Default::default()
        },
        ..grant
    };
    let approval = store
        .grant_task_approval(&ctx, &project.id, &task.id, &grant)
        .unwrap();
    let other = store.create_task(&agent, &project.id, &create).unwrap();
    assert!(
        store
            .transition_task(
                &agent,
                &project.id,
                &other.id,
                "finish",
                &TaskTransitionRequest {
                    expected_revision: other.revision,
                    approval_id: Some(approval.id.clone()),
                    ..Default::default()
                }
            )
            .is_err(),
        "approval for another task is invalid"
    );
    assert!(
        store
            .transition_task(
                &agent,
                &project.id,
                &task.id,
                "finish",
                &TaskTransitionRequest {
                    expected_revision: changed.revision,
                    approval_id: Some(approval.id.clone()),
                    resolution: Some("Unreviewed parameters".into()),
                    ..Default::default()
                }
            )
            .is_err(),
        "exact approval does not permit altering approved parameters"
    );
    let finished = store
        .transition_task(
            &agent,
            &project.id,
            &task.id,
            "finish",
            &TaskTransitionRequest {
                expected_revision: changed.revision,
                approval_id: Some(approval.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(finished.state_id.as_deref(), Some("done"));
    let history = store
        .task_history(&agent, &project.id, &task.id, None, 50)
        .unwrap();
    assert!(history.items.iter().any(
        |event| event.operation == "task.approval_granted" && event.actor_id == ctx.actor_id()
    ));
    assert!(
        history
            .items
            .iter()
            .any(|event| event.operation == "task.approval_consumed"
                && event.actor_id == agent.actor_id())
    );
    assert!(
        store
            .transition_task(
                &agent,
                &project.id,
                &task.id,
                "finish",
                &TaskTransitionRequest {
                    expected_revision: finished.revision,
                    approval_id: Some(approval.id),
                    ..Default::default()
                }
            )
            .is_err()
    );
}
