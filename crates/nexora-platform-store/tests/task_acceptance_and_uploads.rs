#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use task_pool_fixture::TaskPoolFixture;

#[test]
#[ignore = "requires PostgreSQL; multiple criteria persist without becoming completion gates"]
fn multiple_criteria_are_versioned_documentation_only() {
    let mut f = TaskPoolFixture::new(false);
    let mut task = f
        .work
        .store
        .create_pool_task(
            &f.manager,
            &PoolTaskCreate {
                project_id: None,
                definition: TaskCreate {
                    title: "Acceptance".into(),
                    type_code: "task".into(),
                    origin: "test".into(),
                    assignee_id: Some(f.developer.actor_id().into()),
                    acceptance_criteria: vec![" **First** ".into(), "Second".into()],
                    ..Default::default()
                },
            },
        )
        .unwrap();
    assert_eq!(task.acceptance_criteria, vec!["**First**", "Second"]);
    task = f
        .work
        .store
        .change_pool_task(
            &f.manager,
            &task.id,
            &TaskChange {
                expected_revision: task.revision,
                title: Some("Renamed".into()),
                reason: "Rename".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(task.acceptance_criteria.len(), 2);
    assert!(
        f.work
            .store
            .change_pool_task(
                &f.manager,
                &task.id,
                &TaskChange {
                    expected_revision: task.revision,
                    acceptance_criteria: Some(vec![" ".into()]),
                    reason: "Invalid".into(),
                    ..Default::default()
                }
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
    assert_eq!(task.acceptance_criteria.len(), 2);
    task = f
        .work
        .store
        .change_pool_task(
            &f.manager,
            &task.id,
            &TaskChange {
                expected_revision: task.revision,
                acceptance_criteria: Some(vec![]),
                reason: "Clear".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(task.acceptance_criteria.is_empty());
}

#[test]
#[ignore = "requires PostgreSQL; attached documents and images inherit task access"]
fn uploads_are_stored_and_protected_by_task_scope() {
    let mut f = TaskPoolFixture::new(false);
    let task = f.create(
        Some(f.developer.actor_id().into()),
        Some(f.work.project.id.clone()),
    );
    let file = f
        .work
        .store
        .upload_task_document(&f.manager, &task.id, "evidence.pdf", b"%PDF-evidence")
        .unwrap();
    let (_, bytes) = f
        .work
        .store
        .task_document_content(&f.developer, file["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(bytes, b"%PDF-evidence");
    let png = b"\x89PNG\r\n\x1a\nnormalized-test-payload";
    let image = f
        .work
        .store
        .upload_task_image(&f.manager, &task.id, "image.png", png)
        .unwrap();
    assert_eq!(
        f.work
            .store
            .task_evidence_content(&f.developer, image["id"].as_str().unwrap())
            .unwrap(),
        png
    );
    assert!(
        image["markdown"]
            .as_str()
            .unwrap()
            .contains(image["url"].as_str().unwrap())
    );
    assert!(
        f.work
            .store
            .upload_task_image(&f.manager, &task.id, "bad", b"not an image")
            .is_err()
    );
    let page = f
        .work
        .store
        .task_document_page(&f.developer, Some(&task.id), None, "", None)
        .unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    f.work
        .store
        .change_pool_task(
            &f.manager,
            &task.id,
            &TaskChange {
                expected_revision: task.revision,
                archived: Some(true),
                reason: "Archive".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        f.work
            .store
            .upload_task_image(&f.manager, &task.id, "image.png", png)
            .is_err()
    );
    assert!(
        f.work
            .store
            .upload_task_document(&f.manager, &task.id, "new.pdf", b"pdf")
            .is_err()
    );
}
