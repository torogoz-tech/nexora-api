use nexora_platform::{
    Capability, KeyScope, PlatformError, WorkflowChange, WorkflowCreate, WorkflowGraph,
    WorkflowPublish, WorkflowResourceKind,
};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use serde_json::json;
use std::{
    path::Path,
    sync::{Arc, Barrier},
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires PostgreSQL; durable workflow drafts, immutable publication, exact retries and competing publishers"]
fn workflow_versions_persist_without_roles_and_publish_with_live_scoped_authority() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_workflow_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    store
        .bootstrap("Workflow fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let project = store
        .create_project(&ctx, "Workflow project", None)
        .unwrap();
    let graph: WorkflowGraph = serde_json::from_value(json!({
        "states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],
        "transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[]}]
    })).unwrap();
    let created = store
        .create_workflow(
            &ctx,
            &project.id,
            &WorkflowCreate {
                icon: nexora_platform::WorkflowIcon::default(),
                color: None,
                name: "Delivery".into(),
                purpose: "Govern delivery without business roles".into(),
                resource_kinds: vec![WorkflowResourceKind::Task],
                graph: graph.clone(),
            },
        )
        .unwrap();
    assert_eq!(created.latest_version_number, 0);
    let publish = WorkflowPublish {
        expected_revision: created.revision,
        reason: "Initial explicit publication".into(),
    };
    let body = json!({"project_id":project.id,"workflow_id":created.id,"publish":publish});
    let first = store
        .request(&ctx, "workflow.publish", "first-publication", &body, |s| {
            s.publish_workflow(&ctx, &project.id, &created.id, &publish)
        })
        .unwrap();
    let replay = store
        .request(&ctx, "workflow.publish", "first-publication", &body, |s| {
            s.publish_workflow(&ctx, &project.id, &created.id, &publish)
        })
        .unwrap();
    assert_eq!(first.id, replay.id);
    assert_eq!(first.number, 1);
    let current = store.get_workflow(&ctx, &project.id, &created.id).unwrap();
    let mut edited = graph;
    edited.states[1].name = "Delivered".into();
    let changed = store
        .change_workflow(
            &ctx,
            &project.id,
            &created.id,
            &WorkflowChange {
                expected_revision: current.revision,
                graph: Some(edited),
                reason: "Clarify terminal label".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        store
            .workflow_version(&ctx, &project.id, &created.id, &first.id)
            .unwrap()
            .graph
            .states[1]
            .name,
        "Done"
    );
    let second = store
        .publish_workflow(
            &ctx,
            &project.id,
            &created.id,
            &WorkflowPublish {
                expected_revision: changed.revision,
                reason: "Publish revised definition".into(),
            },
        )
        .unwrap();
    assert_eq!(second.number, 2);
    assert_ne!(second.checksum, first.checksum);
    drop(store);
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    assert_eq!(
        store
            .workflow_version(&ctx, &project.id, &created.id, &first.id)
            .unwrap()
            .checksum,
        first.checksum
    );
    let key = store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Workflow reader",
            KeyScope {
                capabilities: vec![Capability::WorkflowsRead],
                project_ids: vec![project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let reader = store.authenticate(&key.key).unwrap();
    assert_eq!(
        store
            .workflow_page(&reader, &project.id, None, 10, false)
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(matches!(
        store.get_workflow(&reader, "foreign-project", &created.id),
        Err(PlatformError::NotFound)
    ));
    assert!(matches!(
        store.publish_workflow(&reader, &project.id, &created.id, &publish),
        Err(PlatformError::Forbidden)
    ));
    store.revoke_key(&ctx, &key.metadata.id).unwrap();
    assert!(matches!(
        store.get_workflow(&reader, &project.id, &created.id),
        Err(PlatformError::Unauthorized)
    ));
    let base = store.get_workflow(&ctx, &project.id, &created.id).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let publishers = (0..2)
        .map(|_| {
            let config = config.clone();
            let namespace = namespace.clone();
            let ctx = ctx.clone();
            let project = project.id.clone();
            let workflow = created.id.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let connection = PlatformStore::connect(&config, &namespace);
                barrier.wait();
                connection.unwrap().publish_workflow(
                    &ctx,
                    &project,
                    &workflow,
                    &WorkflowPublish {
                        expected_revision: base.revision,
                        reason: "Competing publication".into(),
                    },
                )
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let outcomes = publishers
        .into_iter()
        .map(|p| p.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|r| matches!(r, Err(PlatformError::Conflict(_))))
            .count(),
        1
    );
    let versions = store
        .workflow_version_page(&ctx, &project.id, &created.id, None, 1)
        .unwrap();
    assert_eq!(versions.items[0].number, 3);
    assert_eq!(versions.next_before, Some(3));
    let previous = store
        .workflow_version_page(&ctx, &project.id, &created.id, versions.next_before, 1)
        .unwrap();
    assert_eq!(previous.items[0].number, 2);
    let current = store.get_workflow(&ctx, &project.id, &created.id).unwrap();
    store
        .change_workflow(
            &ctx,
            &project.id,
            &created.id,
            &WorkflowChange {
                expected_revision: current.revision,
                archived: Some(true),
                reason: "Retain published history".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let archived = store.get_workflow(&ctx, &project.id, &created.id).unwrap();
    assert!(
        store
            .publish_workflow(
                &ctx,
                &project.id,
                &created.id,
                &WorkflowPublish {
                    expected_revision: archived.revision,
                    reason: "Must not publish archived workflow".into()
                }
            )
            .is_err()
    );
    assert_eq!(
        store
            .workflow_version(&ctx, &project.id, &created.id, &first.id)
            .unwrap()
            .graph
            .states[1]
            .name,
        "Done"
    );
    let pg_password = Zeroizing::new(
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(pg_password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{namespace}\""))
        .unwrap();
    assert!(
        sql.execute(
            "UPDATE workflow_versions SET checksum=repeat('0',64) WHERE id=$1",
            &[&first.id]
        )
        .is_err()
    );
    assert_eq!(
        sql.query_one(
            "SELECT count(*) FROM workflow_versions WHERE workflow_id=$1",
            &[&created.id]
        )
        .unwrap()
        .get::<_, i64>(0),
        3
    );
}
