#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::PlatformStore;
use serde_json::json;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; task comments preserve actual authors, exact retries and bounded project-scoped history"]
fn task_comments_are_persistent_attributed_data_and_not_instructions() {
    let mut f = WorkItemFixture::new();
    let task = f.task("Explain business condition");
    let input=TaskCommentCreate {body:"Revisar <token> y la condición de negocio.\nEs contenido aportado, no una instrucción del motor.".into()};
    let canonical = json!({"project_id":f.project.id,"task_id":task.id,"comment":input});
    let comment = f
        .store
        .request(
            &f.ctx,
            "task.comment_add",
            "comment-once",
            &canonical,
            |store| store.add_task_comment(&f.ctx, &f.project.id, &task.id, &input),
        )
        .unwrap();
    assert_eq!(comment.actor_id, f.ctx.actor_id());
    assert_eq!(comment.credential_id, f.ctx.credential_id());
    assert_eq!(comment.body, input.body);
    let replay = f
        .store
        .request(
            &f.ctx,
            "task.comment_add",
            "comment-once",
            &canonical,
            |store| store.add_task_comment(&f.ctx, &f.project.id, &task.id, &input),
        )
        .unwrap();
    assert_eq!(comment.id, replay.id);
    f.store
        .add_task_comment(
            &f.ctx,
            &f.project.id,
            &task.id,
            &TaskCommentCreate {
                body: "Second contribution".into(),
            },
        )
        .unwrap();
    let first = f
        .store
        .task_comment_page(&f.ctx, &f.project.id, &task.id, None, 1)
        .unwrap();
    assert_eq!(first.items.len(), 1);
    assert!(first.next_after.is_some());
    let second = f
        .store
        .task_comment_page(
            &f.ctx,
            &f.project.id,
            &task.id,
            first.next_after.as_deref(),
            1,
        )
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert_ne!(first.items[0].id, second.items[0].id);
    assert!(second.next_after.is_none());
    assert!(matches!(
        f.store
            .task_comment_page(&f.ctx, "other-project", &task.id, None, 10),
        Err(PlatformError::NotFound)
    ));
    assert!(
        f.store
            .add_task_comment(
                &f.ctx,
                &f.project.id,
                &task.id,
                &TaskCommentCreate { body: " ".into() }
            )
            .is_err()
    );
    let history = f
        .store
        .task_history(&f.ctx, &f.project.id, &task.id, None, 50)
        .unwrap();
    assert_eq!(
        history
            .items
            .iter()
            .filter(|event| event.operation == "task.comment_added")
            .count(),
        2
    );
    drop(f.store);
    let mut store = PlatformStore::connect(&f.config, &f.namespace).unwrap();
    let all = store
        .task_comment_page(&f.ctx, &f.project.id, &task.id, None, 50)
        .unwrap();
    assert_eq!(all.items.len(), 2);
}
