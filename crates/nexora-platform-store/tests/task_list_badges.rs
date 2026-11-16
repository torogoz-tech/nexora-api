#[path = "support/git_author_fixture.rs"]
mod git_author_fixture;
use git_author_fixture::GitAuthorFixture;
use nexora_platform::VisibleTaskQuery;
#[test]
#[ignore = "requires PostgreSQL"]
fn visible_tasks_include_the_pinned_workflow_state_color() {
    let mut f = GitAuthorFixture::new();
    let workflow=f.work.store.create_workflow(&f.work.ctx,&f.work.project.id,&serde_json::from_value(serde_json::json!({"name":"Colored delivery","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true,"color":"#4398AC"},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","from":"ready","to":"done"}]}})).unwrap()).unwrap();
    let version = f
        .work
        .store
        .publish_workflow(
            &f.work.ctx,
            &f.work.project.id,
            &workflow.id,
            &nexora_platform::WorkflowPublish {
                expected_revision: workflow.revision,
                reason: "Test colors".into(),
            },
        )
        .unwrap();
    f.work.backlog = f
        .work
        .store
        .create_backlog(
            &f.work.ctx,
            &f.work.project.id,
            &nexora_platform::BacklogCreate {
                name: "Colored".into(),
                description: None,
                color: None,
                workflow_version_id: version.id,
            },
        )
        .unwrap();
    let task = f.work.task("Colored workflow task");
    let page = f
        .work
        .store
        .visible_tasks(&f.work.ctx, &VisibleTaskQuery::default())
        .unwrap();
    let visible = page
        .items
        .iter()
        .find(|item| item.task.id == task.id)
        .unwrap();
    assert_eq!(visible.state_color.as_ref().unwrap().as_str(), "#4398AC");
    assert_eq!(visible.state_name.as_deref(), Some("Ready"));
}
