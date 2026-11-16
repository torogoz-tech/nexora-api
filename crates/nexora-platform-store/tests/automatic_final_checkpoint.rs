#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
#[path = "support/task_test_sql.rs"]
mod test_sql;
use nexora_platform::*;
use serde_json::json;
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL"]
fn automatic_fin_checkpoints_without_changing_the_completed_stage_revision() {
    let mut f = TaskPoolFixture::new(false);
    let flow=f.work.store.create_developer_workflow(&f.work.ctx,&serde_json::from_value(json!({"name":"Final checkpoint","graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[]},{"id":"final","name":"Final","from":"done","to":"done","automatic":true,"guards":[],"steps":[{"kind":"advance_state"},{"kind":"notify","channel":"in_app"}]}]}})).unwrap()).unwrap();
    let version = f
        .work
        .store
        .publish_developer_workflow(
            &f.work.ctx,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Test".into(),
            },
        )
        .unwrap();
    f.work
        .store
        .assign_actor_workflow(
            &f.work.ctx,
            f.developer.actor_id(),
            &DeveloperWorkflowChange {
                expected_revision: 1,
                workflow_version_id: version.id.clone(),
                reason: "Test".into(),
            },
        )
        .unwrap();
    f.work
        .store
        .create_catalog_task_type(
            &f.work.ctx,
            &CatalogTaskTypeCreate {
                code: "checkpoint_work".into(),
                name: "Checkpoint work".into(),
                purpose: TaskPurpose::Implementation,
                description: None,
                color: None,
                icon: TaskTypeIcon::default(),
            },
        )
        .unwrap();
    let task = f
        .work
        .store
        .create_pool_task(
            &f.work.ctx,
            &PoolTaskCreate {
                project_id: None,
                definition: TaskCreate {
                    title: "Checkpointed work".into(),
                    type_code: "checkpoint_work".into(),
                    entry_workflow_id: Some(flow.id),
                    origin: "test".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
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
    let fin = f
        .work
        .store
        .transition_pool_task(
            &f.developer,
            &task.id,
            "finish",
            &TaskTransitionRequest {
                expected_revision: active.revision,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(fin.revision, active.revision + 1);
    assert_eq!(fin.state_id.as_deref(), Some("done"));
    assert_eq!(fin.lifecycle, TaskLifecycle::Active);
    let stored = f.work.store.pool_task(&f.developer, &task.id).unwrap();
    assert_eq!(stored.revision, fin.revision);
    let mut sql = test_sql::connect(&f.work.config, &f.work.namespace);
    let record: serde_json::Value = sql
        .query_one(
            "SELECT record FROM task_transition_runs WHERE task_id=$1 AND transition_id='final'",
            &[&task.id],
        )
        .unwrap()
        .get(0);
    assert_eq!(record["status"], "succeeded");
    assert_eq!(record["advanced"], true);
    assert_eq!(record["result"]["revision"], fin.revision);
    let count:i64=sql.query_one("SELECT count(*) FROM task_history WHERE task_id=$1 AND operation='task.final_pipeline_started'",&[&task.id]).unwrap().get(0);
    assert_eq!(count, 1);
}
