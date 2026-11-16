#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; global catalog and project selection"]
fn catalog_types_are_available_independently_of_project_selection() {
    let mut f = WorkItemFixture::new();
    let task = f.task("Existing task");
    let bug = f
        .store
        .create_catalog_task_type(
            &f.ctx,
            &CatalogTaskTypeCreate {
                purpose: TaskPurpose::General,
                code: "defect_test".into(),
                name: "Error".into(),
                description: None,
                color: None,
                icon: TaskTypeIcon::Bug,
            },
        )
        .unwrap();
    let selection = f
        .store
        .project_task_type_selection(&f.ctx, &f.project.id)
        .unwrap();
    let updated = f
        .store
        .change_project_task_type_selection(
            &f.ctx,
            &f.project.id,
            &ProjectTaskTypeSelectionChange {
                codes: vec![bug.code.clone()],
                expected_revision: selection.revision,
            },
        )
        .unwrap();
    assert_eq!(updated.codes, vec!["defect_test"]);
    assert!(f.store.get_task(&f.ctx, &f.project.id, &task.id).is_ok());
    assert!(matches!(
        f.store.change_project_task_type_selection(
            &f.ctx,
            &f.project.id,
            &ProjectTaskTypeSelectionChange {
                codes: vec![],
                expected_revision: selection.revision
            }
        ),
        Err(PlatformError::Conflict(_))
    ));
    let types = f
        .store
        .task_type_page(&f.ctx, &f.project.id, None, 100, false)
        .unwrap();
    assert!(types.items.iter().any(|kind| kind.code == "task"));
    assert!(types.items.iter().any(|kind| kind.code == "defect_test"));
    f.store
        .create_task(
            &f.ctx,
            &f.project.id,
            &TaskCreate {
                title: "Global type".into(),
                type_code: "task".into(),
                origin: "test".into(),
                backlog_id: Some(f.backlog.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    let options = f
        .store
        .task_pool_options(&f.ctx, Some(&f.project.id))
        .unwrap();
    assert!(
        options["types"]
            .as_array()
            .unwrap()
            .iter()
            .any(|kind| kind["code"] == "task")
    );
    let fresh = f
        .store
        .create_catalog_task_type(
            &f.ctx,
            &CatalogTaskTypeCreate {
                purpose: TaskPurpose::General,
                code: "global_only".into(),
                name: "Global only".into(),
                description: None,
                color: None,
                icon: TaskTypeIcon::Bug,
            },
        )
        .unwrap();
    f.store
        .create_task(
            &f.ctx,
            &f.project.id,
            &TaskCreate {
                title: "No project type mapping".into(),
                type_code: fresh.code,
                origin: "test".into(),
                backlog_id: Some(f.backlog.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    let changed = f
        .store
        .change_catalog_task_type(
            &f.ctx,
            "defect_test",
            &CatalogTaskTypeChange {
                expected_revision: bug.revision,
                purpose: Some(TaskPurpose::Implementation),
                name: Some("Defecto".into()),
                reason: "Rename".into(),
                description: None,
                archived: None,
                color: ColorChange::default(),
                icon: Some(TaskTypeIcon::Wrench),
            },
        )
        .unwrap();
    assert_eq!(changed.name, "Defecto");
    assert_eq!(changed.icon, TaskTypeIcon::Wrench);
    assert_eq!(changed.purpose, TaskPurpose::Implementation);
    assert_eq!(
        f.store
            .task_type_page(&f.ctx, &f.project.id, None, 100, false)
            .unwrap()
            .items
            .into_iter()
            .find(|kind| kind.code == "defect_test")
            .unwrap()
            .name,
        "Defecto"
    );
    let actor = f
        .store
        .create_actor(&f.ctx, ActorKind::Agent, "Limited", "Read catalog")
        .unwrap();
    f.store
        .grant(
            &f.ctx,
            &actor.id,
            Capability::TasksRead,
            Some(&f.project.id),
        )
        .unwrap();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            &actor.id,
            "Reader",
            KeyScope {
                capabilities: vec![Capability::TasksRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let reader = f.store.authenticate(&key.key).unwrap();
    assert!(f.store.catalog_task_types(&reader, None, 100).is_ok());
    assert!(matches!(
        f.store.create_catalog_task_type(
            &reader,
            &CatalogTaskTypeCreate {
                purpose: TaskPurpose::General,
                code: "blocked".into(),
                name: "Blocked".into(),
                description: None,
                color: None,
                icon: TaskTypeIcon::ListTodo,
            }
        ),
        Err(PlatformError::Forbidden)
    ));
}

#[test]
#[ignore = "requires PostgreSQL; manager catalog changes preserve task purpose snapshots"]
fn manager_can_change_type_purpose_without_reclassifying_existing_tasks() {
    let mut f = WorkItemFixture::new();
    let old_task = f.task("Keep existing execution purpose");
    let password =
        zeroize::Zeroizing::new(nexora_platform_store::SecretGenerator::token("fixture").unwrap());
    let actor = f
        .store
        .create_user(&f.ctx, "catalog_manager", &password, "Catalog manager")
        .unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &actor.id,
            &MemberRoleChange {
                role: MemberRole::Manager,
                all_projects: true,
                project_ids: vec![],
                expected_revision: 0,
            },
        )
        .unwrap();
    let login = f.store.login("catalog_manager", &password).unwrap();
    let gate = f
        .store
        .identity_gate(&login.access_token, "skip", "")
        .unwrap();
    let manager = f
        .store
        .authenticate(gate["access_token"].as_str().unwrap())
        .unwrap();
    let original = f
        .store
        .catalog_task_types(&manager, None, 100)
        .unwrap()
        .items
        .into_iter()
        .find(|t| t.code == "task")
        .unwrap();
    let changed = f.store.change_catalog_task_type(&manager, "task", &serde_json::from_value(serde_json::json!({
        "expected_revision": original.revision, "purpose": "implementation", "reason": "Use development workflow for future work"
    })).unwrap()).unwrap();
    assert_eq!(changed.purpose, TaskPurpose::Implementation);
    assert_eq!(
        f.store
            .get_task(&f.ctx, &f.project.id, &old_task.id)
            .unwrap()
            .purpose,
        TaskPurpose::General
    );
    let new_task = f
        .store
        .create_pool_task(
            &f.ctx,
            &PoolTaskCreate {
                project_id: Some(f.project.id.clone()),
                definition: TaskCreate {
                    title: "New task uses configured purpose".into(),
                    type_code: "task".into(),
                    origin: "test".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    assert_eq!(new_task.purpose, TaskPurpose::Implementation);
    let preserved = f.store.change_catalog_task_type(&manager, "task", &serde_json::from_value(serde_json::json!({
        "expected_revision": changed.revision, "name": "Implementation task", "reason": "Rename only"
    })).unwrap()).unwrap();
    assert_eq!(preserved.purpose, TaskPurpose::Implementation);
    assert!(matches!(
        f.store.change_catalog_task_type(
            &manager,
            "task",
            &serde_json::from_value(serde_json::json!({
                "expected_revision": original.revision, "purpose": "general", "reason": "Stale edit"
            }))
            .unwrap()
        ),
        Err(PlatformError::Conflict(_))
    ));
}
