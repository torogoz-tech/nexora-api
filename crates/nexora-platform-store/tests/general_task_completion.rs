#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL"]
fn general_tasks_resume_without_workflow_and_complete_only_by_their_active_assignee() {
    let mut f = TaskPoolFixture::new(false);
    let task = f.create(Some(f.developer.actor_id().into()), None);
    let active = f
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
    assert!(active.workflow_id.is_none());
    let paused = f
        .work
        .store
        .set_task_paused(&f.developer, &task.id, active.revision, true)
        .unwrap();
    assert!(matches!(
        f.work
            .store
            .complete_general_task(&f.developer, &task.id, paused.revision),
        Err(PlatformError::Conflict(_))
    ));
    let active = f
        .work
        .store
        .set_task_paused(&f.developer, &task.id, paused.revision, false)
        .unwrap();
    assert!(matches!(
        f.work
            .store
            .complete_general_task(&f.other, &task.id, active.revision),
        Err(PlatformError::Forbidden)
    ));
    assert!(matches!(
        f.work
            .store
            .complete_general_task(&f.developer, &task.id, active.revision - 1),
        Err(PlatformError::Conflict(_))
    ));
    let done = f
        .work
        .store
        .complete_general_task(&f.developer, &task.id, active.revision)
        .unwrap();
    assert_eq!(done.lifecycle, TaskLifecycle::Completed);
    assert!(matches!(
        f.work
            .store
            .complete_general_task(&f.developer, &task.id, done.revision),
        Err(PlatformError::Conflict(_))
    ));
}
