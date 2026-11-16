#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; actor-related task visibility, historical assignment, exact counts and key scope"]
fn task_navigation_counts_and_lists_share_actor_visibility() {
    let mut f = WorkItemFixture::new();
    let flow=f.store.create_developer_workflow(&f.ctx,&serde_json::from_value(json!({"name":"Team workflow","graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[]}]}})).unwrap()).unwrap();
    let version = f
        .store
        .publish_developer_workflow(
            &f.ctx,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Ready".into(),
            },
        )
        .unwrap();
    let mut actors = Vec::new();
    let password =
        zeroize::Zeroizing::new(nexora_platform_store::SecretGenerator::token("password").unwrap());
    for (index, name) in ["Developer", "Other developer", "Third developer"]
        .into_iter()
        .enumerate()
    {
        let actor = f
            .store
            .create_user(&f.ctx, &format!("developer{index}"), &password, name)
            .unwrap();
        f.store
            .change_member_role(
                &f.ctx,
                &actor.id,
                &MemberRoleChange {
                    role: MemberRole::Developer,
                    all_projects: false,
                    project_ids: vec![f.project.id.clone()],
                    expected_revision: 0,
                },
            )
            .unwrap();
        f.store
            .assign_actor_workflow(
                &f.ctx,
                &actor.id,
                &DeveloperWorkflowChange {
                    expected_revision: 0,
                    workflow_version_id: version.id.clone(),
                    reason: "Join".into(),
                },
            )
            .unwrap();
        actors.push(actor);
    }
    let key = f
        .store
        .issue_key(
            &f.ctx,
            &actors[0].id,
            "Developer scoped key",
            KeyScope {
                capabilities: vec![
                    Capability::TasksRead,
                    Capability::TasksWrite,
                    Capability::WorkflowsRead,
                ],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let developer = f.store.authenticate(&key.key).unwrap();
    let create = |name: &str, actor: &str| TaskCreate {
        title: name.into(),
        type_code: "task".into(),
        assignee_id: Some(actor.into()),
        origin: "test".into(),
        ..Default::default()
    };
    let incoming = f
        .store
        .create_developer_task(
            &f.ctx,
            &f.project.id,
            &create("Incoming task", &actors[0].id),
        )
        .unwrap();
    let hidden = f
        .store
        .create_developer_task(
            &f.ctx,
            &f.project.id,
            &create("Other developer private task", &actors[1].id),
        )
        .unwrap();
    let delegated = f
        .store
        .create_developer_task(
            &developer,
            &f.project.id,
            &create("Delegated task", &actors[1].id),
        )
        .unwrap();
    let historical = f
        .store
        .create_developer_task(
            &f.ctx,
            &f.project.id,
            &create("Historical assignment", &actors[0].id),
        )
        .unwrap();
    let reassigned = f
        .store
        .change_task(
            &developer,
            &f.project.id,
            &historical.id,
            &TaskChange {
                expected_revision: historical.revision,
                assignee_id: Some(actors[2].id.clone()),
                assignment_state_id: None,
                assignment_workflow_version_id: None,
                reason: "Developer delegates work".into(),
                ..Default::default()
            },
        )
        .unwrap();
    f.store
        .change_task(
            &f.ctx,
            &f.project.id,
            &historical.id,
            &TaskChange {
                expected_revision: reassigned.revision,
                assignee_id: Some(actors[1].id.clone()),
                assignment_state_id: None,
                assignment_workflow_version_id: None,
                reason: "Later reassignment by another person".into(),
                ..Default::default()
            },
        )
        .unwrap();
    // Unrelated tasks cannot be made visible through an edit operation.
    assert!(matches!(
        f.store.change_task(
            &developer,
            &f.project.id,
            &hidden.id,
            &TaskChange {
                expected_revision: hidden.revision,
                title: Some("Still private".into()),
                reason: "No assignment".into(),
                ..Default::default()
            }
        ),
        Err(PlatformError::NotFound)
    ));
    f.store
        .create_task_type(
            &f.ctx,
            &f.project.id,
            &TaskTypeCreate {
                purpose: nexora_platform::TaskPurpose::General,
                code: "bug".into(),
                name: "Errores".into(),
                description: None,
                color: None,
            },
        )
        .unwrap();
    let types = f.store.visible_task_types(&developer, None, 100).unwrap();
    assert_eq!(types.total, 3);
    assert_eq!(
        types.items.iter().find(|t| t.code == "task").unwrap().total,
        3
    );
    assert_eq!(
        types.items.iter().find(|t| t.code == "bug").unwrap().total,
        0
    );
    let mut ids = Vec::new();
    let mut query = VisibleTaskQuery {
        limit: 1,
        ..Default::default()
    };
    loop {
        let page = f.store.visible_tasks(&developer, &query).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.participants.iter().map(|p| p.total).sum::<i64>(), 3);
        assert_eq!(
            page.participants
                .iter()
                .find(|p| p.actor_id == actors[1].id)
                .unwrap()
                .total,
            2
        );
        ids.extend(page.items.into_iter().map(|t| t.task.id));
        if page.next_after.is_none() {
            break;
        }
        query.after = page.next_after;
    }
    ids.sort();
    let mut expected = vec![
        incoming.id.clone(),
        delegated.id.clone(),
        historical.id.clone(),
    ];
    expected.sort();
    assert_eq!(ids, expected);
    assert!(matches!(
        f.store.get_task(&developer, &f.project.id, &hidden.id),
        Err(PlatformError::NotFound)
    ));
    assert!(matches!(
        f.store
            .task_history(&developer, &f.project.id, &hidden.id, None, 100),
        Err(PlatformError::NotFound)
    ));
    assert_eq!(
        f.store
            .task_page(&developer, &f.project.id, &TaskListQuery::default())
            .unwrap()
            .items
            .len(),
        3
    );
    let searched = f
        .store
        .visible_tasks(
            &developer,
            &VisibleTaskQuery {
                search: Some("Historical".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(searched.total, 1);
    assert_eq!(searched.participants.len(), 1);
    assert_eq!(searched.participants[0].actor_id, actors[1].id);
    assert_eq!(searched.participants[0].total, 1);
    let empty = f
        .store
        .visible_tasks(
            &developer,
            &VisibleTaskQuery {
                search: Some("No matching task".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(empty.total, 0);
    assert!(empty.participants.is_empty());
    for (id, name) in [
        (&incoming.id, "developer0"),
        (&hidden.id, "developer1"),
        (&delegated.id, "developer1"),
        (&historical.id, "developer1"),
    ] {
        let session = f.store.login(name, &password).unwrap();
        let ready = f
            .store
            .identity_gate(&session.access_token, "skip", "")
            .unwrap();
        let executor = f
            .store
            .authenticate(ready["access_token"].as_str().unwrap())
            .unwrap();
        let current = f.store.get_task(&f.ctx, &f.project.id, id).unwrap();
        f.store
            .take_task(
                &executor,
                id,
                &TaskTake {
                    expected_revision: current.revision,
                    workflow_id: None,
                },
                Some(&f.project.id),
            )
            .unwrap();
    }

    assert_eq!(
        f.store
            .visible_tasks(
                &developer,
                &VisibleTaskQuery {
                    workflow_id: Some(flow.id.clone()),
                    ..Default::default()
                }
            )
            .unwrap()
            .total,
        3
    );
    assert_eq!(
        f.store
            .visible_tasks(&f.ctx, &VisibleTaskQuery::default())
            .unwrap()
            .total,
        4
    );
    // A broader coordinator credential reads all project tasks; the developer key stays restricted.
    assert_eq!(
        f.store
            .visible_task_types(&developer, None, 100)
            .unwrap()
            .total,
        3
    );
    let manager_key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Project manager",
            KeyScope {
                capabilities: vec![Capability::TasksRead, Capability::ProjectsWrite],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let manager = f.store.authenticate(&manager_key.key).unwrap();
    assert_eq!(
        f.store
            .visible_tasks(&manager, &VisibleTaskQuery::default())
            .unwrap()
            .total,
        4
    );
    let foreign = f
        .store
        .create_project(&f.ctx, "Outside key scope", None)
        .unwrap();
    f.store
        .create_task_type(
            &f.ctx,
            &foreign.id,
            &TaskTypeCreate {
                purpose: nexora_platform::TaskPurpose::General,
                code: "task".into(),
                name: "Task".into(),
                description: None,
                color: None,
            },
        )
        .unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &actors[0].id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: true,
                project_ids: vec![],
                expected_revision: 1,
            },
        )
        .unwrap();
    f.store
        .create_developer_task(
            &f.ctx,
            &foreign.id,
            &create("Outside project task", &actors[0].id),
        )
        .unwrap();
    assert_eq!(
        f.store
            .visible_tasks(&developer, &VisibleTaskQuery::default())
            .unwrap()
            .total,
        3
    );
    assert_eq!(
        f.store
            .visible_task_types(&developer, None, 100)
            .unwrap()
            .total,
        3
    );
    assert_eq!(
        f.store
            .visible_tasks(
                &developer,
                &VisibleTaskQuery {
                    project_id: Some(foreign.id.clone()),
                    ..Default::default()
                }
            )
            .unwrap()
            .total,
        0
    );
    let other_flow = f
        .store
        .create_developer_workflow(
            &f.ctx,
            &DeveloperWorkflowCreate {
                icon: nexora_platform::WorkflowIcon::default(),
                color: None,
                name: "Restricted workflow".into(),
                purpose: String::new(),
                graph: flow.draft.clone(),
            },
        )
        .unwrap();
    assert!(matches!(
        f.store.visible_tasks(
            &developer,
            &VisibleTaskQuery {
                workflow_id: Some(other_flow.id),
                ..Default::default()
            }
        ),
        Err(PlatformError::NotFound)
    ));
    f.store
        .change_member_role(
            &f.ctx,
            &actors[0].id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![foreign.id.clone()],
                expected_revision: 2,
            },
        )
        .unwrap();
    assert_eq!(
        f.store
            .visible_tasks(&developer, &VisibleTaskQuery::default())
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        f.store
            .visible_task_types(&developer, None, 100)
            .unwrap()
            .total,
        0
    );
}

#[test]
#[ignore = "requires PostgreSQL"]
fn task_page_includes_public_member_identities() {
    let mut f = WorkItemFixture::new();
    let task = f.task("Identity presentation");
    let page = f
        .store
        .visible_tasks(&f.ctx, &VisibleTaskQuery::default())
        .unwrap();
    let item = page
        .items
        .iter()
        .find(|item| item.task.id == task.id)
        .unwrap();
    let creator = item
        .creator
        .as_ref()
        .expect("creator included in task page");
    assert_eq!(creator.id, item.created_by);
    assert_eq!(creator.name, "Owner");
    assert!(creator.position.is_empty());
    assert_eq!(creator.avatar_revision, 0);
    assert!(item.assignee.is_none());
}
