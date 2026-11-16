#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::PlatformStore;
use serde_json::json;
use std::sync::{Arc, Barrier};
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; competing initial bindings must never place one developer in two workflows"]
fn competing_memberships_leave_exactly_one_developer_workflow() {
    let mut f = WorkItemFixture::new();
    let actor = f
        .store
        .create_actor(
            &f.ctx,
            ActorKind::Agent,
            "Developer",
            "Concurrent membership",
        )
        .unwrap();
    let input:DeveloperWorkflowCreate=serde_json::from_value(json!({"name":"Concurrent flow","graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done","guards":[]}]}})).unwrap();
    let mut versions = Vec::new();
    for _ in 0..2 {
        let flow = f.store.create_developer_workflow(&f.ctx, &input).unwrap();
        versions.push(
            f.store
                .publish_developer_workflow(
                    &f.ctx,
                    &flow.id,
                    &WorkflowPublish {
                        expected_revision: flow.revision,
                        reason: "Published".into(),
                    },
                )
                .unwrap()
                .id,
        );
    }
    let replacement_version = versions[0].clone();
    let barrier = Arc::new(Barrier::new(3));
    let workers = versions
        .into_iter()
        .map(|version| {
            let config = f.config.clone();
            let namespace = f.namespace.clone();
            let ctx = f.ctx.clone();
            let actor = actor.id.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut store = PlatformStore::connect(&config, &namespace).unwrap();
                barrier.wait();
                store.assign_actor_workflow(
                    &ctx,
                    &actor,
                    &DeveloperWorkflowChange {
                        expected_revision: 0,
                        workflow_version_id: version,
                        reason: "Join".into(),
                    },
                )
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let results = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(PlatformError::Conflict(_))))
            .count(),
        1
    );
    assert_eq!(
        f.store
            .actor_workflow(&f.ctx, &actor.id)
            .unwrap()
            .unwrap()
            .revision,
        1
    );
    f.store
        .remove_actor_workflow(
            &f.ctx,
            &actor.id,
            &DeveloperWorkflowRemoval {
                expected_revision: 1,
                reason: "Remove empty membership".into(),
            },
        )
        .unwrap();
    assert!(f.store.actor_workflow(&f.ctx, &actor.id).unwrap().is_none());
    // Simulate upgrading a previously removed membership: only its audit
    // history exists when migration56 seeds the durable counter.
    let pg_password = zeroize::Zeroizing::new(
        std::fs::read_to_string(f.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&f.config.host)
        .port(f.config.postgres_port)
        .user(&f.config.postgres_user)
        .password(pg_password.trim())
        .dbname(&f.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{}\"", f.namespace))
        .unwrap();
    // Migration30 seeded revision1 directly, without an assignment audit event.
    // Reproduce a seeded membership deleted before the revision-counter upgrade.
    let legacy = f
        .store
        .create_actor(
            &f.ctx,
            ActorKind::Agent,
            "Legacy seeded developer",
            "Pre-counter membership",
        )
        .unwrap();
    sql.execute("INSERT INTO developer_workflows(actor_id,workflow_id,workflow_version_id,assigned_by) SELECT $1,workflow_id,id,$3 FROM workflow_versions WHERE id=$2",
        &[&legacy.id, &replacement_version, &f.ctx.actor_id()]).unwrap();
    f.store
        .remove_actor_workflow(
            &f.ctx,
            &legacy.id,
            &DeveloperWorkflowRemoval {
                expected_revision: 1,
                reason: "Legacy membership removed".into(),
            },
        )
        .unwrap();
    assert_eq!(sql.query_one("SELECT count(*) FROM audit_events WHERE operation='developer.workflow_assigned' AND resource_id=$1", &[&legacy.id]).unwrap().get::<_,i64>(0), 0);
    sql.batch_execute("DROP TABLE developer_workflow_revisions")
        .unwrap();
    sql.batch_execute(include_str!(
        "../migrations/0056_developer_workflow_revisions.sql"
    ))
    .unwrap();
    let replacement = f
        .store
        .assign_actor_workflow(
            &f.ctx,
            &actor.id,
            &DeveloperWorkflowChange {
                expected_revision: 0,
                workflow_version_id: replacement_version.clone(),
                reason: "New membership generation".into(),
            },
        )
        .unwrap();
    assert!(replacement.revision > 1);
    assert!(matches!(
        f.store.assign_actor_workflow(
            &f.ctx,
            &actor.id,
            &DeveloperWorkflowChange {
                expected_revision: 1,
                workflow_version_id: replacement_version.clone(),
                reason: "Stale tab update".into(),
            }
        ),
        Err(PlatformError::Conflict(_))
    ));
    assert!(matches!(
        f.store.remove_actor_workflow(
            &f.ctx,
            &actor.id,
            &DeveloperWorkflowRemoval {
                expected_revision: 1,
                reason: "Stale tab removal".into(),
            }
        ),
        Err(PlatformError::Conflict(_))
    ));
    assert_eq!(
        f.store
            .actor_workflow(&f.ctx, &actor.id)
            .unwrap()
            .unwrap()
            .revision,
        replacement.revision
    );
    let legacy_replacement = f
        .store
        .assign_actor_workflow(
            &f.ctx,
            &legacy.id,
            &DeveloperWorkflowChange {
                expected_revision: 0,
                workflow_version_id: replacement_version.clone(),
                reason: "Legacy rejoin".into(),
            },
        )
        .unwrap();
    assert!(
        legacy_replacement.revision > 1,
        "migration-seeded unaudited revision must never be reissued"
    );
    assert!(matches!(
        f.store.remove_actor_workflow(
            &f.ctx,
            &legacy.id,
            &DeveloperWorkflowRemoval {
                expected_revision: 1,
                reason: "Stale legacy removal".into()
            }
        ),
        Err(PlatformError::Conflict(_))
    ));
    let updated = f
        .store
        .assign_actor_workflow(
            &f.ctx,
            &actor.id,
            &DeveloperWorkflowChange {
                expected_revision: replacement.revision,
                workflow_version_id: replacement_version,
                reason: "Current tab update".into(),
            },
        )
        .unwrap();
    assert_eq!(updated.revision, replacement.revision + 1);
}
