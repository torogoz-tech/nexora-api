#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL; assignment authority is separate from role and workflow editing"]
fn technical_lead_can_assign_workflows_without_editing_roles_or_workflows() {
    let mut f = TaskPoolFixture::new(false);
    let old = f
        .work
        .store
        .actor_workflow(&f.other, f.developer.actor_id())
        .unwrap()
        .unwrap();
    let input: DeveloperWorkflowChange = serde_json::from_value(
        serde_json::json!({"expected_revision":old.revision,"workflow_version_id":f.version.id}),
    )
    .unwrap();
    assert!(
        f.work
            .store
            .assign_actor_workflow(&f.developer, f.other.actor_id(), &input)
            .is_err()
    );
    assert!(
        f.work
            .store
            .assign_actor_workflow(&f.other, f.developer.actor_id(), &input)
            .is_ok()
    );
    assert!(
        !f.work
            .store
            .developer_directory(&f.other, None, 100)
            .unwrap()
            .items
            .is_empty()
    );
    assert!(
        !f.work
            .store
            .developer_workflow_page(&f.other, None, 100, false)
            .unwrap()
            .items
            .is_empty()
    );
    assert!(!f.other.grants().iter().any(|g| matches!(
        g.capability,
        Capability::ActorsManage | Capability::WorkflowsWrite
    )));
    assert!(
        f.work
            .store
            .assign_actor_workflow(&f.other, f.developer.actor_id(), &input)
            .is_err()
    );
}
