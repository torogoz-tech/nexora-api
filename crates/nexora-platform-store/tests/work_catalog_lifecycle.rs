#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; bounded work catalogs, exact revision changes and archived type history"]
fn work_catalogs_preserve_archived_type_references_and_page_by_authorized_project() {
    let mut f = WorkItemFixture::new();
    f.task("Ordinary work remains readable");
    let bug = f
        .store
        .create_task_type(
            &f.ctx,
            &f.project.id,
            &TaskTypeCreate {
                purpose: nexora_platform::TaskPurpose::General,
                code: "bug".into(),
                name: "Bug".into(),
                description: Some("Defect".into()),
                color: None,
            },
        )
        .unwrap();
    let existing=f.store.create_task(&f.ctx,&f.project.id,&serde_json::from_value(json!({"title":"Existing defect","type_code":"bug","workflow_version_id":f.backlog.workflow_version_id,"origin":"Catalog fixture"})).unwrap()).unwrap();
    let first = f
        .store
        .task_type_page(&f.ctx, &f.project.id, None, 1, false)
        .unwrap();
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.items[0].code, "bug");
    assert_eq!(first.next_after.as_deref(), Some("bug"));
    let next = f
        .store
        .task_type_page(&f.ctx, &f.project.id, first.next_after.as_deref(), 1, false)
        .unwrap();
    assert_eq!(next.items[0].code, "task");
    assert!(next.next_after.is_none());
    let change=serde_json::from_value(json!({"expected_revision":bug.revision,"name":"Defect","archived":true,"reason":"Retire old classification"})).unwrap();
    let canonical = json!({"project_id":f.project.id,"type_code":"bug","change":change});
    let archived = f
        .store
        .request(
            &f.ctx,
            "task_type.change",
            "archive-type-once",
            &canonical,
            |store| store.change_task_type(&f.ctx, &f.project.id, "bug", &change),
        )
        .unwrap();
    assert!(archived.archived);
    assert_eq!(archived.code, "bug");
    assert_eq!(archived.revision, bug.revision + 1);
    assert_eq!(
        f.store
            .request(
                &f.ctx,
                "task_type.change",
                "archive-type-once",
                &canonical,
                |store| store.change_task_type(&f.ctx, &f.project.id, "bug", &change)
            )
            .unwrap()
            .revision,
        archived.revision
    );
    assert!(matches!(
        f.store
            .change_task_type(&f.ctx, &f.project.id, "bug", &change),
        Err(PlatformError::Conflict(_))
    ));
    assert_eq!(
        f.store
            .task_type_page(&f.ctx, &f.project.id, None, 100, false)
            .unwrap()
            .items
            .len(),
        1
    );
    assert_eq!(
        f.store
            .task_type_page(&f.ctx, &f.project.id, None, 100, true)
            .unwrap()
            .items
            .len(),
        2
    );
    assert_eq!(
        f.store
            .get_task(&f.ctx, &f.project.id, &existing.id)
            .unwrap()
            .type_code,
        "bug"
    );
    let edited=f.store.change_task(&f.ctx,&f.project.id,&existing.id,&serde_json::from_value(json!({"expected_revision":existing.revision,"title":"Continue existing retired classification","reason":"Existing work can still be maintained"})).unwrap()).unwrap();
    assert_eq!(edited.type_code, "bug");
    assert!(f.store.create_task(&f.ctx,&f.project.id,&serde_json::from_value(json!({"title":"New defect","type_code":"bug","workflow_version_id":f.backlog.workflow_version_id,"origin":"Catalog fixture"})).unwrap()).is_err());
    let restored=f.store.change_task_type(&f.ctx,&f.project.id,"bug",&serde_json::from_value(json!({"expected_revision":archived.revision,"archived":false,"reason":"Restore classification"})).unwrap()).unwrap();
    assert!(!restored.archived);
    f.store.create_task(&f.ctx,&f.project.id,&serde_json::from_value(json!({"title":"New allowed defect","type_code":"bug","workflow_version_id":f.backlog.workflow_version_id,"origin":"Catalog fixture"})).unwrap()).unwrap();
    let other = f
        .store
        .create_backlog(
            &f.ctx,
            &f.project.id,
            &BacklogCreate {
                name: "Retired backlog".into(),
                description: None,
                workflow_version_id: f.backlog.workflow_version_id.clone(),
                color: None,
            },
        )
        .unwrap();
    f.store
        .change_backlog(
            &f.ctx,
            &f.project.id,
            &other.id,
            &BacklogChange {
                expected_revision: other.revision,
                archived: Some(true),
                reason: "Retire empty backlog".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        f.store
            .backlog_page(&f.ctx, &f.project.id, None, 100, false)
            .unwrap()
            .items
            .len(),
        1
    );
    let page = f
        .store
        .backlog_page(&f.ctx, &f.project.id, None, 1, true)
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(page.next_after.is_some());
    let next = f
        .store
        .backlog_page(&f.ctx, &f.project.id, page.next_after.as_deref(), 1, true)
        .unwrap();
    assert_eq!(next.items.len(), 1);
    assert!(next.next_after.is_none());
    assert_ne!(page.items[0].id, next.items[0].id);
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Catalog reader",
            KeyScope {
                capabilities: vec![Capability::TasksRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let reader = f.store.authenticate(&key.key).unwrap();
    assert_eq!(
        f.store
            .task_type_page(&reader, &f.project.id, None, 100, false)
            .unwrap()
            .items
            .len(),
        2
    );
    assert!(matches!(f.store.change_task_type(&reader,&f.project.id,"bug",&serde_json::from_value(json!({"expected_revision":restored.revision,"archived":true,"reason":"Unauthorized archive"})).unwrap()),Err(PlatformError::Forbidden)));
    assert!(matches!(
        f.store
            .backlog_page(&reader, "other-project", None, 10, false),
        Err(PlatformError::NotFound)
    ));
    assert!(matches!(
        f.store
            .task_type_page(&reader, &f.project.id, None, 0, false),
        Err(PlatformError::Validation(_))
    ));
    assert!(matches!(
        f.store
            .backlog_page(&reader, &f.project.id, Some("\n"), 10, false),
        Err(PlatformError::Validation(_))
    ));
    f.store.revoke_key(&f.ctx, &key.metadata.id).unwrap();
    assert!(matches!(
        f.store
            .task_type_page(&reader, &f.project.id, None, 10, false),
        Err(PlatformError::Unauthorized)
    ));
    drop(f.store);
    let mut restarted =
        nexora_platform_store::PlatformStore::connect(&f.config, &f.namespace).unwrap();
    assert_eq!(
        restarted
            .task_type_page(&f.ctx, &f.project.id, None, 100, false)
            .unwrap()
            .items
            .len(),
        2
    );
    assert_eq!(
        restarted
            .backlog_page(&f.ctx, &f.project.id, None, 100, true)
            .unwrap()
            .items
            .len(),
        2
    );
    assert_eq!(
        restarted
            .get_task(&f.ctx, &f.project.id, &existing.id)
            .unwrap()
            .type_code,
        "bug"
    );
}
