#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::PlatformStore;
use serde_json::json;
use std::sync::{Arc, Barrier};
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; validity, concurrent publication, immutable snapshots and large valid template history"]
fn template_periods_publication_and_large_task_updates_preserve_history() {
    let mut f = WorkItemFixture::new();
    let template = f
        .store
        .create_task_template(
            &f.ctx,
            &f.project.id,
            &TaskTemplateCreate {
                code: "large".into(),
                name: "Large context".into(),
            },
        )
        .unwrap();
    let definition = TaskTemplateFields {
        title: Some("Review context".into()),
        context: format!("a{}b", "\n".repeat(7998)),
        type_code: "task".into(),
        priority: TaskPriority::Medium,
        fields: serde_json::from_value(json!({"payload":"x".repeat(15500)})).unwrap(),
    };
    let numeric = f
        .store
        .create_task_template(
            &f.ctx,
            &f.project.id,
            &TaskTemplateCreate {
                code: "numeric".into(),
                name: "Normalized bounds".into(),
            },
        )
        .unwrap();
    let numeric_fields = serde_json::from_value(json!({"numbers":vec![1e18;1300]})).unwrap();
    let numeric_definition = TaskTemplateFields {
        context: String::new(),
        fields: numeric_fields,
        ..definition.clone()
    };
    let result = f.store.publish_task_template_version(
        &f.ctx,
        &f.project.id,
        &numeric.id,
        &TaskTemplatePublish {
            expected_revision: 1,
            definition: numeric_definition,
            valid_from: None,
            valid_until: None,
            reason: "Normalized fields must remain usable as task input".into(),
        },
    );
    assert!(
        matches!(result, Err(PlatformError::Validation(_))),
        "reject a definition whose normalized fields exceed the task budget"
    );
    let expired = f
        .store
        .publish_task_template_version(
            &f.ctx,
            &f.project.id,
            &template.id,
            &TaskTemplatePublish {
                expected_revision: 1,
                definition: definition.clone(),
                valid_from: Some("2000-01-01T00:00:00Z".into()),
                valid_until: Some("2001-01-01T00:00:00Z".into()),
                reason: "Historical validity is not current validity".into(),
            },
        )
        .unwrap();
    let apply = TaskTemplateApply {
        template_version_id: expired.id.clone(),
        backlog_id: Some(f.backlog.id.clone()),
        origin: "External developer".into(),
        ..Default::default()
    };
    assert!(
        f.store
            .apply_task_template(&f.ctx, &f.project.id, &apply)
            .is_err()
    );
    assert!(matches!(
        f.store.publish_task_template_version(
            &f.ctx,
            &f.project.id,
            &template.id,
            &TaskTemplatePublish {
                expected_revision: 1,
                definition: definition.clone(),
                valid_from: None,
                valid_until: None,
                reason: "Stale publication".into()
            }
        ),
        Err(PlatformError::Conflict(_))
    ));
    let current = f
        .store
        .get_task_template(&f.ctx, &f.project.id, &template.id)
        .unwrap();
    let future = f
        .store
        .publish_task_template_version(
            &f.ctx,
            &f.project.id,
            &template.id,
            &TaskTemplatePublish {
                expected_revision: current.revision,
                definition: definition.clone(),
                valid_from: Some("2200-01-01T00:00:00Z".into()),
                valid_until: None,
                reason: "Future version is not yet applicable".into(),
            },
        )
        .unwrap();
    assert!(
        f.store
            .apply_task_template(
                &f.ctx,
                &f.project.id,
                &TaskTemplateApply {
                    template_version_id: future.id,
                    ..apply.clone()
                }
            )
            .is_err()
    );
    let current = f
        .store
        .get_task_template(&f.ctx, &f.project.id, &template.id)
        .unwrap();
    let live = f
        .store
        .publish_task_template_version(
            &f.ctx,
            &f.project.id,
            &template.id,
            &TaskTemplatePublish {
                expected_revision: current.revision,
                definition: definition.clone(),
                valid_from: None,
                valid_until: None,
                reason: format!("a{}b", "\n".repeat(3998)),
            },
        )
        .unwrap();
    let task = f
        .store
        .apply_task_template(
            &f.ctx,
            &f.project.id,
            &TaskTemplateApply {
                template_version_id: live.id.clone(),
                ..apply
            },
        )
        .unwrap();
    let plain = f.task("Explicit work still has no automatic template");
    assert!(plain.template_version_id.is_none() && plain.context.is_empty());
    let oversized = TaskCreate {
        title: "Normalized input".into(),
        type_code: "task".into(),
        backlog_id: Some(f.backlog.id.clone()),
        origin: "Fixture".into(),
        fields: serde_json::from_value(json!({"numbers":vec![1e18;1300]})).unwrap(),
        ..Default::default()
    };
    assert!(
        matches!(
            f.store.create_task(&f.ctx, &f.project.id, &oversized),
            Err(PlatformError::Validation(_))
        ),
        "do not create a task whose stored fields would be impossible to edit"
    );
    let stored_overflow = TaskCreate {
        fields: serde_json::from_value(json!({"numbers":vec![1e308;200]})).unwrap(),
        ..oversized.clone()
    };
    assert!(
        matches!(
            f.store.create_task(&f.ctx, &f.project.id, &stored_overflow),
            Err(PlatformError::Validation(_))
        ),
        "validate stored jsonb size before a database constraint fails"
    );
    let changed = f
        .store
        .change_task(
            &f.ctx,
            &f.project.id,
            &task.id,
            &TaskChange {
                expected_revision: task.revision,
                priority: Some(TaskPriority::High),
                reason: "Large valid context still permits normal task edits".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(changed.revision, task.revision + 1);
    assert!(changed.template_snapshot == task.template_snapshot);
    let history = f
        .store
        .task_history(&f.ctx, &f.project.id, &task.id, None, 50)
        .unwrap();
    assert!(
        history
            .items
            .iter()
            .any(|item| item.operation == "task.changed")
    );
    let policy=f.store.create_workflow(&f.ctx,&f.project.id,&serde_json::from_value(json!({"name":"Template provenance","purpose":"Canonical template reference is a server fact","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[{"kind":"required_field","field":"template_version_id"}]}]}})).unwrap()).unwrap();
    let policy = f
        .store
        .publish_workflow(
            &f.ctx,
            &f.project.id,
            &policy.id,
            &WorkflowPublish {
                expected_revision: policy.revision,
                reason: "Require actual applied template".into(),
            },
        )
        .unwrap();
    let password = zeroize::Zeroizing::new(
        nexora_platform_store::SecretGenerator::token("template_developer").unwrap(),
    );
    let actor = f
        .store
        .create_user(
            &f.ctx,
            "template_developer",
            &password,
            "Template developer",
        )
        .unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &actor.id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![f.project.id.clone()],
                expected_revision: 0,
            },
        )
        .unwrap();
    let execution = f
        .store
        .create_developer_workflow(
            &f.ctx,
            &DeveloperWorkflowCreate {
                name: "Template provenance execution".into(),
                icon: WorkflowIcon::default(),
                color: None,
                purpose: "Validate canonical template references after taking a task".into(),
                graph: policy.graph.clone(),
            },
        )
        .unwrap();
    let execution = f
        .store
        .publish_developer_workflow(
            &f.ctx,
            &execution.id,
            &WorkflowPublish {
                expected_revision: execution.revision,
                reason: "Execute applied templates".into(),
            },
        )
        .unwrap();
    f.store
        .assign_actor_workflow(
            &f.ctx,
            &actor.id,
            &DeveloperWorkflowChange {
                expected_revision: 0,
                workflow_version_id: execution.id.clone(),
                reason: "Execute template provenance checks".into(),
            },
        )
        .unwrap();
    let session = f.store.login("template_developer", &password).unwrap();
    let developer = f.store.authenticate(&session.access_token).unwrap();
    let fake = f
        .store
        .create_task(
            &f.ctx,
            &f.project.id,
            &TaskCreate {
                title: "Untrusted custom field".into(),
                type_code: "task".into(),
                workflow_version_id: Some(policy.id.clone()),
                origin: "Fixture".into(),
                fields: serde_json::from_value(json!({"template_version_id":"forged"})).unwrap(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(fake.lifecycle, TaskLifecycle::Active);
    assert!(
        f.store
            .transition_task(
                &f.ctx,
                &f.project.id,
                &fake.id,
                "finish",
                &TaskTransitionRequest {
                    expected_revision: fake.revision,
                    ..Default::default()
                }
            )
            .is_err(),
        "custom fields cannot forge the canonical template reference"
    );
    let actual = f
        .store
        .apply_task_template(
            &f.ctx,
            &f.project.id,
            &TaskTemplateApply {
                template_version_id: live.id.clone(),
                workflow_version_id: Some(policy.id),
                assignee_id: Some(actor.id.clone()),
                origin: "Fixture".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let actual = f
        .store
        .take_task(
            &developer,
            &actual.id,
            &TaskTake {
                expected_revision: actual.revision,
                workflow_id: None,
            },
            Some(&f.project.id),
        )
        .unwrap();
    assert_eq!(actual.lifecycle, TaskLifecycle::Active);
    assert_eq!(
        f.store
            .transition_task(
                &developer,
                &f.project.id,
                &actual.id,
                "finish",
                &TaskTransitionRequest {
                    expected_revision: actual.revision,
                    ..Default::default()
                }
            )
            .unwrap()
            .state_id,
        Some("done".into())
    );
    let race = f
        .store
        .create_task_template(
            &f.ctx,
            &f.project.id,
            &TaskTemplateCreate {
                code: "race".into(),
                name: "Concurrent publication".into(),
            },
        )
        .unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let mut threads = Vec::new();
    for _ in 0..2 {
        let mut store = PlatformStore::connect(&f.config, &f.namespace).unwrap();
        let barrier = barrier.clone();
        let ctx = f.ctx.clone();
        let project = f.project.id.clone();
        let id = race.id.clone();
        let definition = definition.clone();
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            store.publish_task_template_version(
                &ctx,
                &project,
                &id,
                &TaskTemplatePublish {
                    expected_revision: 1,
                    definition,
                    valid_from: None,
                    valid_until: None,
                    reason: "One expected revision, one winner".into(),
                },
            )
        }));
    }
    barrier.wait();
    let outcomes = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Err(PlatformError::Conflict(_))))
            .count(),
        1
    );
    let password =
        std::fs::read_to_string(f.config.secrets_directory.join("postgres_password")).unwrap();
    let mut client = postgres::Config::new()
        .host(&f.config.host)
        .port(f.config.postgres_port)
        .user(&f.config.postgres_user)
        .password(password.trim())
        .dbname(&f.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    client
        .batch_execute(&format!(
            "SET search_path TO \"{}\",pg_catalog",
            f.namespace
        ))
        .unwrap();
    let error = client
        .execute(
            "UPDATE task_template_versions SET name='tampered' WHERE id=$1",
            &[&live.id],
        )
        .unwrap_err();
    assert_eq!(
        error.code(),
        Some(&postgres::error::SqlState::CHECK_VIOLATION)
    );
    let error = client
        .execute(
            "UPDATE tasks SET template_snapshot='{}'::jsonb WHERE id=$1",
            &[&task.id],
        )
        .unwrap_err();
    assert_eq!(
        error.code(),
        Some(&postgres::error::SqlState::CHECK_VIOLATION)
    );
    let invalid_id = format!("invalid_{}", task.id);
    let error=client.execute("INSERT INTO tasks(id,project_id,title,context,type_code,priority,workflow_id,workflow_version_id,state_id,origin,created_by,template_version_id,template_snapshot) SELECT $1,project_id,title,'',type_code,priority,workflow_id,workflow_version_id,state_id,origin,created_by,template_version_id,jsonb_build_object('id',NULL,'project_id',project_id) FROM tasks WHERE id=$2",&[&invalid_id,&task.id]).unwrap_err();
    assert_eq!(
        error.code(),
        Some(&postgres::error::SqlState::CHECK_VIOLATION)
    );
}
