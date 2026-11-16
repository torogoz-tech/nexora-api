#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use serde_json::json;
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL"]
fn policy_entrypoint_runs_final_pipeline_without_revising_terminal_task_twice() {
    let mut f = TaskPoolFixture::new(false);
    let flow=f.work.store.create_developer_workflow(&f.work.ctx,&serde_json::from_value(json!({"name":"Automatic final","graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","from":"ready","to":"done","steps":[{"kind":"advance_state"}]},{"id":"deliver","from":"done","to":"done","automatic":true,"steps":[{"kind":"advance_state"},{"kind":"notify","channel":"in_app"}]}]}})).unwrap()).unwrap();
    let version = f
        .work
        .store
        .publish_developer_workflow(
            &f.work.ctx,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Test pipeline".into(),
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
                workflow_version_id: version.id,
                reason: "Test pipeline".into(),
            },
        )
        .unwrap();
    f.work
        .store
        .create_catalog_task_type(
            &f.work.ctx,
            &CatalogTaskTypeCreate {
                code: "pipeline_work".into(),
                name: "Implementation".into(),
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
                    title: "Final pipeline".into(),
                    type_code: "pipeline_work".into(),
                    entry_workflow_id: Some(flow.id),
                    origin: "test".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let task = f
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
    let before = task.revision;
    let task = f
        .work
        .store
        .transition_pool_task_with_policy(
            &f.developer,
            &task.id,
            "finish",
            &TaskTransitionRequest {
                expected_revision: before,
                ..Default::default()
            },
            &nexora_platform_store::GitNetworkPolicy::default(),
        )
        .unwrap();
    assert_eq!(task.lifecycle, TaskLifecycle::Active);
    assert_eq!(task.revision, before + 1);
    let runs = f
        .work
        .store
        .task_transition_runs(&f.developer, &task.id)
        .unwrap();
    assert!(runs.to_string().contains("deliver"));
    let finish = runs["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|run| run["transition_id"] == "finish")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let retried = f
        .work
        .store
        .retry_task_transition(
            &f.developer,
            &task.id,
            &finish,
            &nexora_platform_store::GitNetworkPolicy::default(),
        )
        .unwrap();
    assert_eq!(retried.revision, task.revision);
    let runs = f
        .work
        .store
        .task_transition_runs(&f.developer, &task.id)
        .unwrap();
    assert_eq!(
        runs["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|run| run["transition_id"] == "deliver")
            .count(),
        1
    );
    assert_eq!(
        f.work
            .store
            .task_stages(&f.developer, &task.id)
            .unwrap()
            .len(),
        1
    );
}
