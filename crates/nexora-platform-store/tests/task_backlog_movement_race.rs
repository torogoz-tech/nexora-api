#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::PlatformStore;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; concurrent movement, backlog reordering and dependency mutation share a consistent lock order"]
fn movement_ordering_and_dependency_writes_finish_without_deadlock_or_duplicate_ranks() {
    let mut f = WorkItemFixture::new();
    let a = f.task("Moving task");
    let b = f.task("Ordering anchor");
    let source = f
        .store
        .get_backlog(&f.ctx, &f.project.id, &f.backlog.id)
        .unwrap();
    let destination = f
        .store
        .create_backlog(
            &f.ctx,
            &f.project.id,
            &BacklogCreate {
                name: "Concurrent destination".into(),
                description: None,
                workflow_version_id: source.workflow_version_id.clone(),
                color: None,
            },
        )
        .unwrap();
    let barrier = std::sync::Barrier::new(3);
    let results = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for branch in 0..3 {
            let config = &f.config;
            let namespace = &f.namespace;
            let ctx = &f.ctx;
            let project = &f.project.id;
            let source = &source;
            let destination = &destination;
            let a = &a;
            let b = &b;
            let barrier = &barrier;
            handles.push(scope.spawn(move||{
    let mut store=PlatformStore::connect(config,namespace).unwrap();barrier.wait();
    match branch {
     0=>store.move_task(ctx,project,&a.id,&serde_json::from_value(json!({"expected_revision":a.revision,"backlog_id":destination.id,"expected_source_backlog_revision":source.revision,"expected_target_backlog_revision":destination.revision,"reason":"Concurrent explicit movement"})).unwrap()).map(|_|()),
     1=>store.reorder_backlog(ctx,project,&source.id,&BacklogReorder{expected_revision:source.revision,task_id:b.id.clone(),before_id:Some(a.id.clone()),after_id:None}).map(|_|()),
     _=>store.create_task_dependency(ctx,project,&TaskDependencyCreate{source_task_id:a.id.clone(),target_task_id:b.id.clone(),kind:TaskDependencyKind::Blocks,source_revision:a.revision,target_revision:b.revision,reason:"Concurrent dependency condition".into()}).map(|_|())
    }
   }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(results.iter().any(Result::is_ok));
    for result in results {
        assert!(
            matches!(result, Ok(()) | Err(PlatformError::Conflict(_))),
            "Unexpected race result: {result:?}"
        );
    }
    let actual_a = f.store.get_task(&f.ctx, &f.project.id, &a.id).unwrap();
    let actual_b = f.store.get_task(&f.ctx, &f.project.id, &b.id).unwrap();
    assert_eq!(actual_a.workflow_version_id, a.workflow_version_id);
    assert_eq!(actual_a.state_id, a.state_id);
    if actual_a.backlog_id == actual_b.backlog_id {
        assert_ne!(actual_a.position, actual_b.position);
    }
    let history = f
        .store
        .task_history(&f.ctx, &f.project.id, &a.id, None, 100)
        .unwrap();
    assert!(
        history
            .items
            .iter()
            .filter(|item| item.operation == "task.moved")
            .count()
            <= 1
    );
}
