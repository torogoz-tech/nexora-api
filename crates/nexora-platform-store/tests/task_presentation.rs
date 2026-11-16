#[path = "support/feature_map_fixture.rs"]
mod feature_map_fixture;
use feature_map_fixture::FeatureMapFixture;
use nexora_platform::*;
#[test]
#[ignore = "requires PostgreSQL; task-scoped business traceability and attenuated knowledge access"]
fn task_page_respects_task_visibility_and_separate_document_authority() {
    let mut f = FeatureMapFixture::new();
    let input = f.proposal("Authentication", true);
    let proposal = f
        .work
        .store
        .propose_feature_map(&f.developer, &f.work.project.id, &input)
        .unwrap();
    let approved = f
        .work
        .store
        .decide_feature_map(
            &f.pmo,
            &f.work.project.id,
            proposal["id"].as_str().unwrap(),
            &FeatureMapFixture::decision(true),
        )
        .unwrap();
    let id = approved["result"]["task_ids"][0].as_str().unwrap();
    let page = f
        .work
        .store
        .pool_task_presentation(&f.developer, id)
        .unwrap();
    assert_eq!(page["id"], id);
    assert_eq!(page["lifecycle"], "pending");
    assert!(page["workflow_id"].is_null());
    assert_eq!(page["assignee_name"], "Developer");
    assert_eq!(page["knowledge"]["can_read"], true);
    assert_eq!(page["knowledge"]["feature"]["name"], "Authentication");
    assert_eq!(page["knowledge"]["documents"][0]["id"], f.requirement.id);
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.developer.actor_id(),
            "Task reader",
            KeyScope {
                capabilities: vec![Capability::TasksRead],
                project_ids: vec![f.work.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let attenuated = f.work.store.authenticate(&key.key).unwrap();
    let page = f
        .work
        .store
        .pool_task_presentation(&attenuated, id)
        .unwrap();
    assert_eq!(page["knowledge"]["can_read"], false);
    assert!(page["knowledge"]["feature"].is_null());
    assert_eq!(page["knowledge"]["documents"], serde_json::json!([]));
    let hidden = f.work.task("Private work");
    assert!(matches!(
        f.work
            .store
            .pool_task_presentation(&f.developer, &hidden.id),
        Err(PlatformError::NotFound)
    ));
    let projectless=f.work.store.create_pool_task(&f.work.ctx,&serde_json::from_value(serde_json::json!({"project_id":null,"title":"Shared work","type_code":"task","origin":"fixture","fields":{}})).unwrap()).unwrap();
    let shared = f
        .work
        .store
        .pool_task_presentation(&f.developer, &projectless.id)
        .unwrap();
    assert!(shared["project_id"].is_null());
    assert!(shared["knowledge"]["feature"].is_null());
    assert_eq!(shared["created_by_name"], "Owner");
    assert_eq!(shared["creator"]["name"], "Owner");
    assert_eq!(shared["creator"]["id"], projectless.created_by);
    assert_eq!(shared["creator"]["avatar_revision"], 0);
    let page = f
        .work
        .store
        .visible_tasks(&f.developer, &VisibleTaskQuery::default())
        .unwrap();
    let initial = page
        .items
        .iter()
        .find(|t| t.task.id == projectless.id)
        .unwrap()
        .state_changed_at
        .clone();
    assert!(initial.is_some());
    f.work
        .store
        .change_pool_task(
            &f.work.ctx,
            &projectless.id,
            &TaskChange {
                expected_revision: projectless.revision,
                title: Some("Renamed without a state change".into()),
                reason: "Rename".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let page = f
        .work
        .store
        .visible_tasks(&f.developer, &VisibleTaskQuery::default())
        .unwrap();
    let renamed = page
        .items
        .iter()
        .find(|t| t.task.id == projectless.id)
        .unwrap();
    assert_eq!(renamed.state_changed_at, initial);
    let before_take = renamed.task.revision;
    f.work
        .store
        .take_task(
            &f.developer,
            &projectless.id,
            &TaskTake {
                expected_revision: before_take,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    let page = f
        .work
        .store
        .visible_tasks(&f.developer, &VisibleTaskQuery::default())
        .unwrap();
    let taken = page
        .items
        .iter()
        .find(|t| t.task.id == projectless.id)
        .unwrap();
    assert_ne!(taken.state_changed_at, initial);
    assert!(taken.state_changed_at.is_some());
}
