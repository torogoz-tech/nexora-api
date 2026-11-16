#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; explicit backlog movement preserves pinned flow, optimistic revisions and durable history"]
fn moving_tasks_changes_backlogs_without_adopting_the_destination_flow() {
    let mut f = WorkItemFixture::new();
    let task = f.task("Keep its original workflow");
    let workflow=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Destination policy","purpose":"Defaults apply only to new work","resource_kinds":["task"],"graph":{"states":[{"id":"queued","name":"Queued","initial":true},{"id":"closed","name":"Closed","terminal":true}],"transitions":[{"id":"close","name":"Close","from":"queued","to":"closed","guards":[{"kind":"reason"}]}]}})).unwrap()).unwrap();
    let flow = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &workflow.id,
            &WorkflowPublish {
                expected_revision: workflow.revision,
                reason: "Publish destination default".into(),
            },
        )
        .unwrap();
    let target = f
        .store
        .create_backlog(
            &f.ctx,
            &f.project.id,
            &BacklogCreate {
                name: "Destination".into(),
                description: None,
                workflow_version_id: flow.id.clone(),
                color: None,
            },
        )
        .unwrap();
    let resident=f.store.create_task(&f.ctx,&f.project.id,&serde_json::from_value(json!({"title":"Existing target task","type_code":"task","backlog_id":target.id,"origin":"Movement fixture"})).unwrap()).unwrap();
    assert_eq!(resident.workflow_version_id, Some(flow.id));
    let source = f
        .store
        .get_backlog(&f.ctx, &f.project.id, &f.backlog.id)
        .unwrap();
    let target = f
        .store
        .get_backlog(&f.ctx, &f.project.id, &target.id)
        .unwrap();
    let definition = json!({"expected_revision":task.revision,"backlog_id":target.id,"expected_source_backlog_revision":source.revision,"expected_target_backlog_revision":target.revision,"reason":"Organize work without changing policy"});
    let mut invalid = definition.clone();
    invalid["expected_target_backlog_revision"] = json!(target.revision + 1);
    assert!(matches!(
        f.store.move_task(
            &f.ctx,
            &f.project.id,
            &task.id,
            &serde_json::from_value(invalid).unwrap()
        ),
        Err(PlatformError::Conflict(_))
    ));
    assert_eq!(
        f.store
            .get_task(&f.ctx, &f.project.id, &task.id)
            .unwrap()
            .backlog_id,
        task.backlog_id
    );
    assert_eq!(
        f.store
            .get_backlog(&f.ctx, &f.project.id, &source.id)
            .unwrap()
            .revision,
        source.revision
    );
    let movement = serde_json::from_value(definition).unwrap();
    let canonical = json!({"project_id":f.project.id,"task_id":task.id,"movement":movement});
    let moved = f
        .store
        .request(&f.ctx, "task.move", "move-once", &canonical, |store| {
            store.move_task(&f.ctx, &f.project.id, &task.id, &movement)
        })
        .unwrap();
    assert_eq!(moved.backlog_id.as_deref(), Some(target.id.as_str()));
    assert_eq!(moved.workflow_version_id, task.workflow_version_id);
    assert_eq!(moved.workflow_id, task.workflow_id);
    assert_eq!(moved.state_id, task.state_id);
    assert_eq!(moved.origin, task.origin);
    assert_eq!(moved.revision, task.revision + 1);
    assert!(moved.position > resident.position);
    assert_eq!(
        f.store
            .request(&f.ctx, "task.move", "move-once", &canonical, |store| store
                .move_task(&f.ctx, &f.project.id, &task.id, &movement))
            .unwrap()
            .revision,
        moved.revision
    );
    assert_eq!(
        f.store
            .get_backlog(&f.ctx, &f.project.id, &source.id)
            .unwrap()
            .revision,
        source.revision + 1
    );
    assert_eq!(
        f.store
            .get_backlog(&f.ctx, &f.project.id, &target.id)
            .unwrap()
            .revision,
        target.revision + 1
    );
    let history = f
        .store
        .task_history(&f.ctx, &f.project.id, &task.id, None, 100)
        .unwrap();
    assert_eq!(
        history
            .items
            .iter()
            .filter(|item| item.operation == "task.moved")
            .count(),
        1
    );
    let outside=f.store.create_task(&f.ctx,&f.project.id,&serde_json::from_value(json!({"title":"Initially outside backlog","type_code":"task","workflow_version_id":task.workflow_version_id,"origin":"Movement fixture"})).unwrap()).unwrap();
    let source = f
        .store
        .get_backlog(&f.ctx, &f.project.id, &source.id)
        .unwrap();
    let placed=f.store.move_task(&f.ctx,&f.project.id,&outside.id,&serde_json::from_value(json!({"expected_revision":outside.revision,"backlog_id":source.id,"expected_target_backlog_revision":source.revision,"reason":"Assign an explicit backlog"})).unwrap()).unwrap();
    assert_eq!(placed.backlog_id.as_deref(), Some(source.id.as_str()));
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Task writer without backlog ordering",
            KeyScope {
                capabilities: vec![Capability::TasksWrite],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let limited = f.store.authenticate(&key.key).unwrap();
    let source = f
        .store
        .get_backlog(&f.ctx, &f.project.id, &source.id)
        .unwrap();
    let target = f
        .store
        .get_backlog(&f.ctx, &f.project.id, &target.id)
        .unwrap();
    assert!(matches!(f.store.move_task(&limited,&f.project.id,&placed.id,&serde_json::from_value(json!({"expected_revision":placed.revision,"backlog_id":target.id,"expected_source_backlog_revision":source.revision,"expected_target_backlog_revision":target.revision,"reason":"Unauthorized backlog change"})).unwrap()),Err(PlatformError::Forbidden)));
    drop(f.store);
    let mut restarted =
        nexora_platform_store::PlatformStore::connect(&f.config, &f.namespace).unwrap();
    let persisted = restarted.get_task(&f.ctx, &f.project.id, &task.id).unwrap();
    assert_eq!(persisted.backlog_id, moved.backlog_id);
    assert_eq!(persisted.workflow_version_id, task.workflow_version_id);
}
