#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use serde_json::json;
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL; explicit takes, cross-project board attenuation and pending reassignment"]
fn developer_workflows_span_projects_and_reassignment_requires_a_new_take() {
    let mut f = TaskPoolFixture::new(false);
    f.work
        .store
        .change_member_role(
            &f.work.ctx,
            f.developer.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: true,
                project_ids: vec![],
                expected_revision: 1,
            },
        )
        .unwrap();
    let second_project = f
        .work
        .store
        .create_project(&f.work.ctx, "Second project", None)
        .unwrap();
    f.work
        .store
        .create_task_type(
            &f.work.ctx,
            &second_project.id,
            &TaskTypeCreate {
                purpose: nexora_platform::TaskPurpose::General,
                code: "task".into(),
                name: "Task".into(),
                description: None,
                color: None,
            },
        )
        .unwrap();
    let first = f.create(
        Some(f.developer.actor_id().into()),
        Some(f.work.project.id.clone()),
    );
    let second = f
        .work
        .store
        .create_pool_task(
            &f.work.ctx,
            &PoolTaskCreate {
                project_id: Some(second_project.id.clone()),
                definition: TaskCreate {
                    title: "Second scope".into(),
                    type_code: "task".into(),
                    assignee_id: Some(f.developer.actor_id().into()),
                    origin: "test".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    assert!(
        f.work
            .store
            .developer_board_tasks(&f.developer, None, 100)
            .unwrap()
            .items
            .is_empty()
    );
    let first = f
        .work
        .store
        .take_task(
            &f.developer,
            &first.id,
            &TaskTake {
                expected_revision: 1,
                workflow_id: None,
            },
            Some(&f.work.project.id),
        )
        .unwrap();
    f.work
        .store
        .take_task(
            &f.developer,
            &second.id,
            &TaskTake {
                expected_revision: 1,
                workflow_id: None,
            },
            Some(&second_project.id),
        )
        .unwrap();
    assert_eq!(
        f.work
            .store
            .developer_board_tasks(&f.developer, None, 100)
            .unwrap()
            .items
            .len(),
        2
    );
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.developer.actor_id(),
            "Scoped board",
            KeyScope {
                capabilities: vec![Capability::TasksRead, Capability::WorkflowsRead],
                project_ids: vec![f.work.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let limited = f.work.store.authenticate(&key.key).unwrap();
    let page = f
        .work
        .store
        .developer_board_tasks(&limited, None, 100)
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(
        page.items[0].task.project_id.as_deref(),
        Some(f.work.project.id.as_str())
    );
    let flow=f.work.store.create_developer_workflow(&f.work.ctx,&serde_json::from_value(json!({"name":"Other execution","graph":{"states":[{"id":"queued","name":"Queued","initial":true},{"id":"closed","name":"Closed","terminal":true}],"transitions":[{"id":"close","name":"Close","from":"queued","to":"closed","guards":[]}]}})).unwrap()).unwrap();
    let version = f
        .work
        .store
        .publish_developer_workflow(
            &f.work.ctx,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Publish".into(),
            },
        )
        .unwrap();
    let assignment = f
        .work
        .store
        .actor_workflow(&f.developer, f.developer.actor_id())
        .unwrap()
        .unwrap();
    assert!(matches!(
        f.work.store.assign_actor_workflow(
            &f.work.ctx,
            f.developer.actor_id(),
            &DeveloperWorkflowChange {
                expected_revision: assignment.revision,
                workflow_version_id: version.id.clone(),
                reason: "Cannot move active work".into()
            }
        ),
        Err(PlatformError::Conflict(_))
    ));
    f.work
        .store
        .assign_actor_workflow(
            &f.work.ctx,
            f.other.actor_id(),
            &DeveloperWorkflowChange {
                expected_revision: 0,
                workflow_version_id: version.id.clone(),
                reason: "Select future execution policy".into(),
            },
        )
        .unwrap();
    let moved = f
        .work
        .store
        .change_task(
            &f.manager,
            &f.work.project.id,
            &first.id,
            &TaskChange {
                expected_revision: first.revision,
                assignee_id: Some(f.other.actor_id().into()),
                reason: "Handoff for explicit take".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(moved.lifecycle, TaskLifecycle::Pending);
    assert!(moved.workflow_version_id.is_none());
    assert!(
        f.work
            .store
            .developer_board_tasks(&f.other, None, 100)
            .unwrap()
            .items
            .is_empty()
    );
    let moved = f
        .work
        .store
        .take_task(
            &f.other,
            &moved.id,
            &TaskTake {
                expected_revision: moved.revision,
                workflow_id: Some(version.workflow_id.clone()),
            },
            Some(&f.work.project.id),
        )
        .unwrap();
    assert_eq!(
        moved.workflow_version_id.as_deref(),
        Some(version.id.as_str())
    );
    assert_eq!(moved.state_id.as_deref(), Some("queued"));
    assert_eq!(
        f.work
            .store
            .developer_board_tasks(&f.developer, None, 100)
            .unwrap()
            .items
            .len(),
        1
    );
    assert_eq!(
        f.work
            .store
            .developer_board_tasks(&f.other, None, 100)
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(matches!(
        f.work.store.remove_actor_workflow(
            &f.work.ctx,
            f.developer.actor_id(),
            &DeveloperWorkflowRemoval {
                expected_revision: assignment.revision,
                reason: "Active second task remains".into()
            }
        ),
        Err(PlatformError::Conflict(_))
    ));
    assert_eq!(
        f.work
            .store
            .developer_workflow_version(&f.work.ctx, &f.version.workflow_id, &f.version.id)
            .unwrap()
            .checksum,
        f.version.checksum
    );
}
