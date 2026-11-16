#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use serde_json::json;
use task_pool_fixture::TaskPoolFixture;

#[test]
#[ignore = "requires PostgreSQL"]
fn technical_lead_keeps_multiple_memberships_and_selects_on_take() {
    let mut f = TaskPoolFixture::new(false);
    let flow = f.work.store.create_developer_workflow(&f.work.ctx, &serde_json::from_value(json!({"name":"QA","graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done"}]}})).unwrap()).unwrap();
    let version = f
        .work
        .store
        .publish_developer_workflow(
            &f.work.ctx,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Published QA".into(),
            },
        )
        .unwrap();
    f.work
        .store
        .assign_actor_workflow(
            &f.work.ctx,
            f.other.actor_id(),
            &DeveloperWorkflowChange {
                expected_revision: 0,
                workflow_version_id: version.id.clone(),
                reason: String::new(),
            },
        )
        .unwrap();
    assert!(
        f.work
            .store
            .developer_workflow_members(&f.work.ctx, &f.version.workflow_id)
            .unwrap()
            .iter()
            .any(|m| m.actor_id == f.other.actor_id())
    );
    let memberships = f
        .work
        .store
        .actor_workflows(&f.other, f.other.actor_id())
        .unwrap();
    assert_eq!(memberships.len(), 2);
    let directory = f
        .work
        .store
        .developer_directory(&f.work.ctx, None, 100)
        .unwrap();
    let member = directory
        .items
        .iter()
        .find(|m| m.id == f.other.actor_id())
        .unwrap();
    assert_eq!(member.workflows.len(), 2);
    assert!(
        member.workflow_id.is_none(),
        "legacy projection must not pick an arbitrary workflow"
    );
    let downgrade = f.work.store.change_member_role(
        &f.work.ctx,
        f.other.actor_id(),
        &MemberRoleChange {
            role: MemberRole::Developer,
            all_projects: false,
            project_ids: vec![f.work.project.id.clone()],
            expected_revision: 1,
        },
    );
    assert!(
        downgrade.is_err(),
        "multiple memberships prevent downgrade to developer"
    );
    let task = f.create(None, None);
    let unassigned = TaskTake {
        expected_revision: task.revision,
        workflow_id: Some("not-assigned".into()),
    };
    assert!(
        f.work
            .store
            .take_task(&f.other, &task.id, &unassigned, None)
            .is_err()
    );
    let missing: TaskTake =
        serde_json::from_value(json!({"expected_revision":task.revision})).unwrap();
    assert!(
        f.work
            .store
            .take_task(&f.other, &task.id, &missing, None)
            .is_err()
    );
    let selected: TaskTake =
        serde_json::from_value(json!({"expected_revision":task.revision,"workflow_id":flow.id}))
            .unwrap();
    let taken = f
        .work
        .store
        .take_task(&f.other, &task.id, &selected, None)
        .unwrap();
    assert_eq!(taken.workflow_id.as_deref(), Some(flow.id.as_str()));
    assert_eq!(
        taken.workflow_version_id.as_deref(),
        Some(version.id.as_str())
    );
    let qa = memberships
        .iter()
        .find(|m| m.workflow_id == flow.id)
        .unwrap();
    assert!(
        f.work
            .store
            .remove_actor_workflow(
                &f.work.ctx,
                f.other.actor_id(),
                &DeveloperWorkflowRemoval {
                    expected_revision: qa.revision,
                    reason: "Remove QA".into()
                }
            )
            .is_err(),
        "cannot remove a workflow with an active task"
    );
    let original = memberships
        .iter()
        .find(|m| m.workflow_id == f.version.workflow_id)
        .unwrap();
    f.work
        .store
        .remove_actor_workflow(
            &f.work.ctx,
            f.other.actor_id(),
            &DeveloperWorkflowRemoval {
                expected_revision: original.revision,
                reason: "Remove unused membership".into(),
            },
        )
        .unwrap();
    let restored = f
        .work
        .store
        .assign_actor_workflow(
            &f.work.ctx,
            f.other.actor_id(),
            &DeveloperWorkflowChange {
                expected_revision: 0,
                workflow_version_id: f.version.id.clone(),
                reason: String::new(),
            },
        )
        .unwrap();
    assert!(restored.revision > qa.revision);
    assert!(
        f.work
            .store
            .remove_actor_workflow(
                &f.work.ctx,
                f.other.actor_id(),
                &DeveloperWorkflowRemoval {
                    expected_revision: original.revision,
                    reason: "Stale removal".into()
                }
            )
            .is_err()
    );
    let second = f.create(None, None);
    let taken_second = f
        .work
        .store
        .take_task(
            &f.other,
            &second.id,
            &TaskTake {
                expected_revision: second.revision,
                workflow_id: Some(f.version.workflow_id.clone()),
            },
            None,
        )
        .unwrap();
    assert_ne!(taken_second.workflow_id, taken.workflow_id);
    let old = f
        .work
        .store
        .actor_workflow(&f.developer, f.developer.actor_id())
        .unwrap()
        .unwrap();
    assert!(
        f.work
            .store
            .assign_actor_workflow(
                &f.work.ctx,
                f.developer.actor_id(),
                &DeveloperWorkflowChange {
                    expected_revision: 0,
                    workflow_version_id: version.id.clone(),
                    reason: String::new()
                }
            )
            .is_err()
    );
    f.work
        .store
        .assign_actor_workflow(
            &f.work.ctx,
            f.developer.actor_id(),
            &DeveloperWorkflowChange {
                expected_revision: old.revision,
                workflow_version_id: version.id.clone(),
                reason: String::new(),
            },
        )
        .unwrap();
    assert_eq!(
        f.work
            .store
            .actor_workflows(&f.developer, f.developer.actor_id())
            .unwrap()
            .len(),
        1
    );
    for actor in [f.manager.clone(), f.work.ctx.clone()] {
        for id in [&f.version.id, &version.id] {
            f.work
                .store
                .assign_actor_workflow(
                    &f.work.ctx,
                    actor.actor_id(),
                    &DeveloperWorkflowChange {
                        expected_revision: 0,
                        workflow_version_id: id.clone(),
                        reason: String::new(),
                    },
                )
                .unwrap();
        }
        assert_eq!(
            f.work
                .store
                .actor_workflows(&actor, actor.actor_id())
                .unwrap()
                .len(),
            2
        );
        let task = f.create(None, None);
        assert!(
            f.work
                .store
                .take_task(
                    &actor,
                    &task.id,
                    &TaskTake {
                        expected_revision: task.revision,
                        workflow_id: None
                    },
                    None
                )
                .is_err()
        );
        f.work
            .store
            .take_task(
                &actor,
                &task.id,
                &TaskTake {
                    expected_revision: task.revision,
                    workflow_id: Some(flow.id.clone()),
                },
                None,
            )
            .unwrap();
    }
    f.work
        .store
        .change_member_role(
            &f.work.ctx,
            f.manager.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Pmo,
                all_projects: false,
                project_ids: vec![f.work.project.id.clone()],
                expected_revision: 1,
            },
        )
        .unwrap();
    let task = f.create(None, None);
    f.work
        .store
        .take_task(
            &f.manager,
            &task.id,
            &TaskTake {
                expected_revision: task.revision,
                workflow_id: Some(f.version.workflow_id.clone()),
            },
            None,
        )
        .unwrap();
}
