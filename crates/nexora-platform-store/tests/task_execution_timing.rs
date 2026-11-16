#[path = "support/feature_map_fixture.rs"]
mod feature_map_fixture;
use feature_map_fixture::FeatureMapFixture;
use nexora_platform::*;
#[test]
#[ignore = "requires PostgreSQL; execution duration across pending reservations and completion"]
fn execution_duration_excludes_pending_periods_and_stops_at_completion() {
    let mut f = FeatureMapFixture::new();
    let mut task=f.work.store.create_pool_task(&f.work.ctx,&serde_json::from_value(serde_json::json!({"project_id":null,"title":"Timed task","type_code":"task","origin":"test","fields":{}})).unwrap()).unwrap();
    task = f
        .work
        .store
        .take_task(
            &f.developer,
            &task.id,
            &TaskTake {
                expected_revision: task.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    task = f
        .work
        .store
        .change_pool_task(
            &f.developer,
            &task.id,
            &TaskChange {
                expected_revision: task.revision,
                title: Some("Title edit while active".into()),
                reason: "Edit".into(),
                ..Default::default()
            },
        )
        .unwrap();
    task = f
        .work
        .store
        .change_pool_task(
            &f.developer,
            &task.id,
            &TaskChange {
                expected_revision: task.revision,
                clear_assignee: true,
                reason: "Return to pending".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(task.lifecycle, TaskLifecycle::Pending);
    task = f
        .work
        .store
        .take_task(
            &f.developer,
            &task.id,
            &TaskTake {
                expected_revision: task.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    task = f
        .work
        .store
        .transition_pool_task(
            &f.developer,
            &task.id,
            "finish",
            &TaskTransitionRequest {
                expected_revision: task.revision,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(task.lifecycle, TaskLifecycle::Completed);
    f.sql.execute("WITH ranked AS(SELECT id,operation,row_number() OVER(PARTITION BY operation ORDER BY sequence) n FROM task_history WHERE task_id=$1) UPDATE task_history h SET created_at=now()-CASE WHEN r.operation='task.created' THEN interval '3 hours' WHEN r.operation='task.taken' AND r.n=1 THEN interval '2 hours' WHEN r.operation='task.changed' THEN interval '90 minutes' WHEN r.operation='task.reassigned_workflow' THEN interval '1 hour' WHEN r.operation='task.taken' THEN interval '30 minutes' ELSE interval '10 minutes' END FROM ranked r WHERE h.id=r.id",&[&task.id]).unwrap();
    let page = f
        .work
        .store
        .pool_task_presentation(&f.developer, &task.id)
        .unwrap();
    assert_eq!(page["timing"]["active_seconds"], 4800, "{}", page["timing"]);
    assert!(page["timing"]["started_at"].is_string());
    assert!(page["timing"]["completed_at"].is_string());
}
