#[path = "support/feature_map_fixture.rs"]
mod feature_map_fixture;
use base64::Engine;
use feature_map_fixture::FeatureMapFixture;
use nexora_platform::*;
use serde_json::json;
#[test]
#[ignore = "requires PostgreSQL; task project transfers preserve identity and history with live scope checks"]
fn task_project_transfer_preserves_contributions_and_rejects_incompatible_links() {
    assert!(
        serde_json::from_value::<TaskProjectChange>(
            json!({"expected_revision":1,"reason":"Missing destination"})
        )
        .is_err()
    );
    let mut f = FeatureMapFixture::new();
    let bytes=base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==").unwrap();
    let evidence = f
        .work
        .store
        .upload_task_evidence(&f.work.ctx, "capture.png", &bytes)
        .unwrap();
    let context = format!("![Evidence](<{}>)", evidence["url"].as_str().unwrap());
    let mut task=f.work.store.create_pool_task(&f.work.ctx,&serde_json::from_value(json!({"project_id":null,"title":"Move independently","type_code":"task","origin":"test","context":context,"assignee_id":f.developer.actor_id(),"fields":{}})).unwrap()).unwrap();
    let comment = f
        .work
        .store
        .add_pool_task_comment(
            &f.work.ctx,
            &task.id,
            &TaskCommentCreate {
                body: "Keep my original contribution".into(),
            },
        )
        .unwrap();
    let before_history = f
        .work
        .store
        .pool_task_history(&f.work.ctx, &task.id, None, 100)
        .unwrap();
    let change = TaskProjectChange {
        expected_revision: task.revision,
        project_id: Some(f.work.project.id.clone()),
        reason: "Assign project".into(),
    };
    let narrow = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "No destination authority",
            KeyScope {
                capabilities: vec![Capability::TasksRead, Capability::TasksWrite],
                project_ids: vec![],
                all_projects: false,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let narrow = f.work.store.authenticate(&narrow.key).unwrap();
    assert!(
        f.work
            .store
            .change_task_project(&narrow, &task.id, &change)
            .is_err()
    );
    let canonical = json!({"task":task.id,"change":change});
    task = f
        .work
        .store
        .request(
            &f.work.ctx,
            "task.project.change",
            "project-transfer",
            &canonical,
            |store| store.change_task_project(&f.work.ctx, &task.id, &change),
        )
        .unwrap();
    assert_eq!(task.project_id, Some(f.work.project.id.clone()));
    assert_eq!(task.lifecycle, TaskLifecycle::Pending);
    assert_eq!(task.context, context);
    assert_eq!(
        f.work
            .store
            .task_evidence_content(&f.work.ctx, evidence["id"].as_str().unwrap())
            .unwrap(),
        bytes
    );
    assert!(
        f.work
            .store
            .task_evidence_content(&narrow, evidence["id"].as_str().unwrap())
            .is_err()
    );
    assert_eq!(task.assignee_id.as_deref(), Some(f.developer.actor_id()));
    let replay = f
        .work
        .store
        .request(
            &f.work.ctx,
            "task.project.change",
            "project-transfer",
            &canonical,
            |store| store.change_task_project(&f.work.ctx, &task.id, &change),
        )
        .unwrap();
    assert_eq!(replay.revision, task.revision);
    let comments = f
        .work
        .store
        .pool_task_comments(&f.work.ctx, &task.id, None, 100)
        .unwrap();
    assert_eq!(comments.items[0].body, comment.body);
    assert_eq!(comments.items[0].id, comment.id);
    assert_eq!(comments.items[0].created_at, comment.created_at);
    let history = f
        .work
        .store
        .pool_task_history(&f.work.ctx, &task.id, None, 100)
        .unwrap();
    assert_eq!(history.items[0].id, before_history.items[0].id);
    assert_eq!(history.items[0].details, before_history.items[0].details);
    assert_eq!(
        history.items.last().unwrap().operation,
        "task.project_changed"
    );
    assert!(
        f.sql
            .execute(
                "UPDATE task_comments SET body='tamper' WHERE id=$1",
                &[&comment.id]
            )
            .is_err()
    );
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
            Some(&f.work.project.id),
        )
        .unwrap();
    let workflow = task.workflow_version_id.clone();
    task = f
        .work
        .store
        .change_task_project(
            &f.work.ctx,
            &task.id,
            &TaskProjectChange {
                expected_revision: task.revision,
                project_id: None,
                reason: "Team work".into(),
            },
        )
        .unwrap();
    assert_eq!(task.workflow_version_id, workflow);
    assert_eq!(task.lifecycle, TaskLifecycle::Active);
    assert_eq!(
        f.work
            .store
            .pool_task_comments(&f.work.ctx, &task.id, None, 100)
            .unwrap()
            .items[0]
            .id,
        comment.id
    );
    let blocker=f.work.store.create_pool_task(&f.work.ctx,&serde_json::from_value(json!({"project_id":null,"title":"Linked task","type_code":"task","origin":"test","fields":{}})).unwrap()).unwrap();
    f.work
        .store
        .create_pool_task_dependency(
            &f.work.ctx,
            &task.id,
            &TaskDependencyCreate {
                source_task_id: task.id.clone(),
                target_task_id: blocker.id,
                source_revision: task.revision,
                target_revision: blocker.revision,
                kind: TaskDependencyKind::RelatedTo,
                reason: "Keep scope coherent".into(),
            },
        )
        .unwrap();
    let current = f.work.store.pool_task(&f.work.ctx, &task.id).unwrap();
    assert!(matches!(
        f.work.store.change_task_project(
            &f.work.ctx,
            &task.id,
            &TaskProjectChange {
                expected_revision: current.revision,
                project_id: Some(f.work.project.id.clone()),
                reason: "Cannot move related task".into()
            }
        ),
        Err(PlatformError::Conflict(_))
    ));
}
