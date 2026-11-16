#[allow(dead_code)]
#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use serde_json::json;
use task_pool_fixture::TaskPoolFixture;

fn sql(f: &TaskPoolFixture) -> postgres::Client {
    let config = &f.work.config;
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut client = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    client
        .batch_execute(&format!("SET search_path TO \"{}\"", f.work.namespace))
        .unwrap();
    client
}

fn package_input(tasks: Vec<String>) -> ProjectContextPackageCreate {
    serde_json::from_value(
        json!({"name":"Bounded task context","task_ids":tasks,"reason":"Capture permitted tasks"}),
    )
    .unwrap()
}

#[test]
#[ignore = "requires PostgreSQL; context capture applies individual task authority"]
fn hidden_tasks_cannot_be_captured_even_with_project_documents_and_task_grants() {
    let mut f = TaskPoolFixture::new(false);
    let hidden = f.create(
        Some(f.other.actor_id().into()),
        Some(f.work.project.id.clone()),
    );
    let visible = f.create(
        Some(f.developer.actor_id().into()),
        Some(f.work.project.id.clone()),
    );
    assert!(matches!(
        f.work
            .store
            .get_task(&f.developer, &f.work.project.id, &hidden.id),
        Err(PlatformError::NotFound)
    ));
    let mut db = sql(&f);
    let before: i64 = db
        .query_one("SELECT count(*) FROM audit_events", &[])
        .unwrap()
        .get(0);
    assert!(matches!(
        f.work.store.create_project_context_package(
            &f.developer,
            &f.work.project.id,
            &package_input(vec![visible.id.clone(), hidden.id.clone()])
        ),
        Err(PlatformError::NotFound)
    ));
    let after: i64 = db
        .query_one("SELECT count(*) FROM audit_events", &[])
        .unwrap()
        .get(0);
    assert_eq!(before, after);
    assert_eq!(
        db.query_one("SELECT count(*) FROM project_context_packages", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    let permitted = f
        .work
        .store
        .create_project_context_package(
            &f.developer,
            &f.work.project.id,
            &package_input(vec![visible.id.clone()]),
        )
        .unwrap();
    assert_eq!(permitted.content.tasks[0].id, visible.id);
    let elevated = f
        .work
        .store
        .create_project_context_package(
            &f.manager,
            &f.work.project.id,
            &package_input(vec![hidden.id]),
        )
        .unwrap();
    assert_eq!(elevated.content.tasks.len(), 1);
    // A key with document permissions cannot inherit its owner's task permission.
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.developer.actor_id(),
            "Document context only",
            KeyScope {
                capabilities: vec![Capability::DocumentsRead],
                project_ids: vec![f.work.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let reader = f.work.store.authenticate(&key.key).unwrap();
    assert!(matches!(
        f.work
            .store
            .get_project_context_package(&reader, &f.work.project.id, &permitted.id),
        Err(PlatformError::NotFound)
    ));
}

#[test]
#[ignore = "requires PostgreSQL; historical package reads, exports and receipt retries use current task visibility"]
fn package_retrieval_export_and_create_replay_recheck_every_captured_task() {
    let mut f = TaskPoolFixture::new(false);
    let first = f.create(
        Some(f.developer.actor_id().into()),
        Some(f.work.project.id.clone()),
    );
    let second = f.create(
        Some(f.developer.actor_id().into()),
        Some(f.work.project.id.clone()),
    );
    let input = package_input(vec![first.id, second.id]);
    let body = json!({"project_id":f.work.project.id,"package":input});
    let package = f
        .work
        .store
        .request(
            &f.developer,
            "context_package.create",
            "context-authority-retry",
            &body,
            |store| store.create_project_context_package(&f.developer, &f.work.project.id, &input),
        )
        .unwrap();
    assert_eq!(
        f.work
            .store
            .request(
                &f.developer,
                "context_package.create",
                "context-authority-retry",
                &body,
                |store| store.create_project_context_package(
                    &f.developer,
                    &f.work.project.id,
                    &input
                )
            )
            .unwrap()
            .id,
        package.id
    );
    // Make freshness inspection return early on the first task, then revoke access
    // to the second. Every task still has to be authorized before returning data.
    let first = &package.content.tasks[0];
    f.work
        .store
        .change_task(
            &f.manager,
            &f.work.project.id,
            &first.id,
            &TaskChange {
                expected_revision: first.revision,
                title: Some("Freshness changed".into()),
                reason: "Retain historical snapshot".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let second = &package.content.tasks[1];
    f.work
        .store
        .change_task(
            &f.manager,
            &f.work.project.id,
            &second.id,
            &TaskChange {
                expected_revision: second.revision,
                clear_assignee: true,
                reason: "Release responsibility".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(matches!(
        f.work
            .store
            .get_task(&f.developer, &f.work.project.id, &second.id),
        Err(PlatformError::NotFound)
    ));
    assert!(matches!(
        f.work
            .store
            .get_project_context_package(&f.developer, &f.work.project.id, &package.id),
        Err(PlatformError::NotFound)
    ));
    for format in ["json", "markdown"] {
        assert!(matches!(
            f.work.store.export_project_context_package(
                &f.developer,
                &f.work.project.id,
                &package.id,
                format
            ),
            Err(PlatformError::NotFound)
        ));
    }
    assert!(matches!(
        f.work.store.request(
            &f.developer,
            "context_package.create",
            "context-authority-retry",
            &body,
            |store| store.create_project_context_package(&f.developer, &f.work.project.id, &input)
        ),
        Err(PlatformError::NotFound)
    ));
    let historical = f
        .work
        .store
        .get_project_context_package(&f.manager, &f.work.project.id, &package.id)
        .unwrap();
    assert!(historical.needs_review);
    assert_eq!(historical.checksum, package.checksum);
    assert!(
        f.work
            .store
            .export_project_context_package(&f.manager, &f.work.project.id, &package.id, "json")
            .unwrap()
            .content
            .contains("Pending pool task")
    );
}

#[test]
#[ignore = "requires PostgreSQL; document link writes and cached retries cannot modify hidden tasks"]
fn document_link_create_remove_and_replay_require_individual_task_access() {
    let mut f = TaskPoolFixture::new(false);
    let hidden = f.create(
        Some(f.other.actor_id().into()),
        Some(f.work.project.id.clone()),
    );
    let task = f.create(
        Some(f.developer.actor_id().into()),
        Some(f.work.project.id.clone()),
    );
    let workflow = f.work.store.create_workflow(&f.work.ctx, &f.work.project.id, &serde_json::from_value(json!({"name":"Documentation","resource_kinds":["requirement"],"graph":{"states":[{"id":"draft","name":"Draft","initial":true}],"transitions":[]}})).unwrap()).unwrap();
    let version = f
        .work
        .store
        .publish_workflow(
            &f.work.ctx,
            &f.work.project.id,
            &workflow.id,
            &WorkflowPublish {
                expected_revision: workflow.revision,
                reason: "Documentation policy".into(),
            },
        )
        .unwrap();
    let doc = f.work.store.create_project_document(&f.work.ctx, &f.work.project.id, &serde_json::from_value(json!({"code":"REQ-VIS","title":"Task prerequisite","content":{"kind":"requirement","body":{"problem":"Visibility","objective":"Protect task prerequisites","scope":"Fixture","business_actors":["Developer"],"acceptance_criteria":["No hidden writes"],"source":"Test"}},"origin":"declared_business","workflow_version_id":version.id,"reason":"Exercise link authorization"})).unwrap()).unwrap();
    let mut db = sql(&f);
    let before: i64 = db
        .query_one("SELECT count(*) FROM audit_events", &[])
        .unwrap()
        .get(0);
    let denied: DocumentLinkCreate = serde_json::from_value(json!({"expected_document_revision":doc.revision,"version_id":doc.latest_version_id,"target":{"kind":"task","task_id":hidden.id,"expected_revision":hidden.revision},"kind":"required_for_transition","reason":"Guess hidden revision"})).unwrap();
    assert!(matches!(
        f.work
            .store
            .create_document_link(&f.developer, &f.work.project.id, &doc.id, &denied),
        Err(PlatformError::NotFound)
    ));
    assert_eq!(
        before,
        db.query_one("SELECT count(*) FROM audit_events", &[])
            .unwrap()
            .get::<_, i64>(0)
    );
    assert_eq!(
        f.work
            .store
            .get_task(&f.manager, &f.work.project.id, &hidden.id)
            .unwrap()
            .revision,
        hidden.revision
    );
    assert_eq!(
        f.work
            .store
            .get_project_document(&f.manager, &f.work.project.id, &doc.id)
            .unwrap()
            .revision,
        doc.revision
    );
    let input: DocumentLinkCreate = serde_json::from_value(json!({"expected_document_revision":doc.revision,"version_id":doc.latest_version_id,"target":{"kind":"task","task_id":task.id,"expected_revision":task.revision},"kind":"required_for_transition","reason":"Permitted prerequisite"})).unwrap();
    let body = json!({"project_id":f.work.project.id,"document_id":doc.id,"link":input});
    let link = f
        .work
        .store
        .request(
            &f.developer,
            "document.link_create",
            "create-link-authority",
            &body,
            |store| store.create_document_link(&f.developer, &f.work.project.id, &doc.id, &input),
        )
        .unwrap();
    assert_eq!(
        f.work
            .store
            .request(
                &f.developer,
                "document.link_create",
                "create-link-authority",
                &body,
                |store| store.create_document_link(
                    &f.developer,
                    &f.work.project.id,
                    &doc.id,
                    &input
                )
            )
            .unwrap()
            .id,
        link.id
    );
    let current = f
        .work
        .store
        .get_task(&f.manager, &f.work.project.id, &task.id)
        .unwrap();
    let released = f
        .work
        .store
        .change_task(
            &f.manager,
            &f.work.project.id,
            &task.id,
            &TaskChange {
                expected_revision: current.revision,
                clear_assignee: true,
                reason: "No longer responsible".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let doc = f
        .work
        .store
        .get_project_document(&f.manager, &f.work.project.id, &doc.id)
        .unwrap();
    let removal = DocumentLinkRemove {
        expected_document_revision: doc.revision,
        expected_target_revision: released.revision,
        reason: "Remove prerequisite".into(),
    };
    let before: i64 = db
        .query_one("SELECT count(*) FROM audit_events", &[])
        .unwrap()
        .get(0);
    assert!(matches!(
        f.work.store.request(
            &f.developer,
            "document.link_create",
            "create-link-authority",
            &body,
            |store| store.create_document_link(&f.developer, &f.work.project.id, &doc.id, &input)
        ),
        Err(PlatformError::NotFound)
    ));
    assert!(matches!(
        f.work.store.remove_document_link(
            &f.developer,
            &f.work.project.id,
            &doc.id,
            &link.id,
            &removal
        ),
        Err(PlatformError::NotFound)
    ));
    assert_eq!(
        before,
        db.query_one("SELECT count(*) FROM audit_events", &[])
            .unwrap()
            .get::<_, i64>(0)
    );
    assert_eq!(
        f.work
            .store
            .get_task(&f.manager, &f.work.project.id, &task.id)
            .unwrap()
            .revision,
        released.revision
    );
    assert_eq!(
        f.work
            .store
            .get_project_document(&f.manager, &f.work.project.id, &doc.id)
            .unwrap()
            .revision,
        doc.revision
    );
    let remove_body = json!({"document_id":doc.id,"link_id":link.id,"removal":removal});
    let removed = f
        .work
        .store
        .request(
            &f.other,
            "document.link_remove",
            "remove-link-authority",
            &remove_body,
            |store| {
                store.remove_document_link(
                    &f.other,
                    &f.work.project.id,
                    &doc.id,
                    &link.id,
                    &removal,
                )
            },
        )
        .unwrap();
    assert!(removed.removed_at.is_some());
    assert!(
        f.work
            .store
            .request(
                &f.other,
                "document.link_remove",
                "remove-link-authority",
                &remove_body,
                |store| store.remove_document_link(
                    &f.other,
                    &f.work.project.id,
                    &doc.id,
                    &link.id,
                    &removal
                )
            )
            .unwrap()
            .removed_at
            .is_some()
    );
    f.work
        .store
        .change_member_role(
            &f.work.ctx,
            f.other.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![f.work.project.id.clone()],
                expected_revision: 1,
            },
        )
        .unwrap();
    let before: i64 = db
        .query_one("SELECT count(*) FROM audit_events", &[])
        .unwrap()
        .get(0);
    assert!(matches!(
        f.work.store.request(
            &f.other,
            "document.link_remove",
            "remove-link-authority",
            &remove_body,
            |store| store.remove_document_link(
                &f.other,
                &f.work.project.id,
                &doc.id,
                &link.id,
                &removal
            )
        ),
        Err(PlatformError::NotFound)
    ));
    assert_eq!(
        before,
        db.query_one("SELECT count(*) FROM audit_events", &[])
            .unwrap()
            .get::<_, i64>(0)
    );
}

#[test]
#[ignore = "requires PostgreSQL; task authorization is checked after waiting for a concurrent reassignment"]
fn context_capture_rechecks_visibility_after_waiting_for_task_lock() {
    let mut f = TaskPoolFixture::new(false);
    let task = f.create(
        Some(f.developer.actor_id().into()),
        Some(f.work.project.id.clone()),
    );
    let mut db = sql(&f);
    let pid: i32 = db.query_one("SELECT pg_backend_pid()", &[]).unwrap().get(0);
    let mut tx = db.transaction().unwrap();
    tx.execute(
        "UPDATE tasks SET assignee_id=NULL,revision=revision+1 WHERE id=$1",
        &[&task.id],
    )
    .unwrap();
    let config = f.work.config.clone();
    let namespace = f.work.namespace.clone();
    let developer = f.developer.clone();
    let project = f.work.project.id.clone();
    let thread = std::thread::spawn(move || {
        let mut store = nexora_platform_store::PlatformStore::connect(&config, &namespace).unwrap();
        store.create_project_context_package(&developer, &project, &package_input(vec![task.id]))
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        tx.query_one("SELECT pg_stat_clear_snapshot()", &[])
            .unwrap();
        let waiting: bool = tx
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
                &[&pid],
            )
            .unwrap()
            .get(0);
        if waiting {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "capture did not wait on the task row"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    tx.commit().unwrap();
    assert!(matches!(
        thread.join().unwrap(),
        Err(PlatformError::NotFound)
    ));
}
