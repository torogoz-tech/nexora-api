#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::PlatformStore;
use serde_json::json;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; explicit template versions apply typed defaults and retain immutable task snapshots"]
fn task_template_versions_are_explicit_and_historical_task_context_survives_archive() {
    let mut f = WorkItemFixture::new();
    let template = f
        .store
        .create_task_template(
            &f.ctx,
            &f.project.id,
            &TaskTemplateCreate {
                code: "triage".into(),
                name: "Incident triage".into(),
            },
        )
        .unwrap();
    let definition = TaskTemplateFields {
        title: Some("Investigate incident".into()),
        context: "Capture reproduction and business impact".into(),
        type_code: "task".into(),
        priority: TaskPriority::High,
        fields: serde_json::from_value(
            json!({"syntax":"Map<K,V>","note":"token is a lexical symbol"}),
        )
        .unwrap(),
    };
    let first = f
        .store
        .publish_task_template_version(
            &f.ctx,
            &f.project.id,
            &template.id,
            &TaskTemplatePublish {
                expected_revision: template.revision,
                definition: definition.clone(),
                valid_from: None,
                valid_until: None,
                reason: "First explicit template".into(),
            },
        )
        .unwrap();
    let plain = f.task("Work without implicit template defaults");
    assert!(plain.template_version_id.is_none() && plain.template_snapshot.is_none());
    assert!(plain.context.is_empty());
    let apply = TaskTemplateApply {
        template_version_id: first.id.clone(),
        title: Some("Customer incident".into()),
        backlog_id: Some(f.backlog.id.clone()),
        origin: "External developer".into(),
        ..Default::default()
    };
    let canonical = json!({"project_id":f.project.id,"application":apply});
    let task = f
        .store
        .request(
            &f.ctx,
            "task.template_apply",
            "template-apply-once",
            &canonical,
            |store| store.apply_task_template(&f.ctx, &f.project.id, &apply),
        )
        .unwrap();

    assert_eq!(task.title, "Customer incident");
    assert_eq!(task.context, definition.context);
    assert_eq!(task.priority, TaskPriority::High);
    assert_eq!(task.template_version_id.as_deref(), Some(first.id.as_str()));
    let snapshot = task
        .template_snapshot
        .clone()
        .expect("template application retains a snapshot");
    assert_eq!(snapshot["checksum"], first.checksum);
    assert_eq!(snapshot["definition"]["context"], definition.context);
    let current = f
        .store
        .get_task_template(&f.ctx, &f.project.id, &template.id)
        .unwrap();
    let mut next_definition = definition.clone();
    next_definition.context = "New defaults never rewrite earlier work".into();
    let second = f
        .store
        .publish_task_template_version(
            &f.ctx,
            &f.project.id,
            &template.id,
            &TaskTemplatePublish {
                expected_revision: current.revision,
                definition: next_definition.clone(),
                valid_from: None,
                valid_until: None,
                reason: "New template defaults".into(),
            },
        )
        .unwrap();
    let current_task = f.store.get_task(&f.ctx, &f.project.id, &task.id).unwrap();
    assert_eq!(current_task.context, definition.context);
    assert_eq!(current_task.template_snapshot, Some(snapshot.clone()));
    let next = f
        .store
        .apply_task_template(
            &f.ctx,
            &f.project.id,
            &TaskTemplateApply {
                template_version_id: second.id.clone(),
                ..apply.clone()
            },
        )
        .unwrap();
    assert_eq!(next.context, next_definition.context);
    let historical = f
        .store
        .task_template_version(&f.ctx, &f.project.id, &template.id, &first.id)
        .unwrap();
    assert_eq!(historical.checksum, first.checksum);
    let page = f
        .store
        .task_template_version_page(&f.ctx, &f.project.id, &template.id, None, 1)
        .unwrap();
    assert_eq!(page.items[0].id, second.id);
    assert!(page.next_before.is_some());
    assert!(matches!(
        f.store.apply_task_template(&f.ctx, "other-project", &apply),
        Err(PlatformError::Forbidden) | Err(PlatformError::NotFound)
    ));
    let current = f
        .store
        .get_task_template(&f.ctx, &f.project.id, &template.id)
        .unwrap();
    f.store
        .change_task_template(
            &f.ctx,
            &f.project.id,
            &template.id,
            &TaskTemplateChange {
                expected_revision: current.revision,
                archived: Some(true),
                reason: "Retire defaults, retain history".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        f.store
            .apply_task_template(&f.ctx, &f.project.id, &apply)
            .is_err()
    );
    let replay = f
        .store
        .request(
            &f.ctx,
            "task.template_apply",
            "template-apply-once",
            &canonical,
            |store| store.apply_task_template(&f.ctx, &f.project.id, &apply),
        )
        .unwrap();
    assert_eq!(
        replay.id, task.id,
        "archiving prevents new applications but does not destroy exact historical receipts"
    );
    assert!(
        f.store
            .task_template_page(&f.ctx, &f.project.id, None, 50, false)
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(
        f.store
            .task_template_page(&f.ctx, &f.project.id, None, 50, true)
            .unwrap()
            .items
            .len(),
        1
    );
    drop(f.store);
    let mut store = PlatformStore::connect(&f.config, &f.namespace).unwrap();
    assert_eq!(
        store
            .get_task(&f.ctx, &f.project.id, &task.id)
            .unwrap()
            .template_snapshot,
        Some(snapshot)
    );
}
