#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use nexora_platform_store::PlatformStore;
use std::sync::{Arc, Barrier};
use task_pool_fixture::TaskPoolFixture;
fn pending(f: &mut TaskPoolFixture, title: &str) -> WorkTask {
    f.work
        .store
        .create_pool_task(
            &f.work.ctx,
            &PoolTaskCreate {
                project_id: Some(f.work.project.id.clone()),
                definition: TaskCreate {
                    title: title.into(),
                    origin: "test".into(),
                    type_code: "task".into(),
                    assignee_id: Some(f.developer.actor_id().into()),
                    ..Default::default()
                },
            },
        )
        .unwrap()
}

#[test]
#[ignore = "requires PostgreSQL; project dependency cycles remain impossible with concurrent disjoint endpoint writes"]
fn task_dependency_guards_and_concurrent_cycle_checks_are_transactional() {
    let mut pool = TaskPoolFixture::new(false);
    let a = pending(&mut pool, "A");
    let b = pending(&mut pool, "B");
    let c = pending(&mut pool, "C");
    let d = pending(&mut pool, "D");
    let left = pending(&mut pool, "Left");
    let right = pending(&mut pool, "Right");
    let developer = pool.developer.clone();
    let f = &mut pool.work;
    let ab = f
        .store
        .create_task_dependency(
            &f.ctx,
            &f.project.id,
            &TaskDependencyCreate {
                source_task_id: a.id.clone(),
                target_task_id: b.id.clone(),
                kind: TaskDependencyKind::Blocks,
                source_revision: a.revision,
                target_revision: b.revision,
                reason: "B waits for A".into(),
            },
        )
        .unwrap();
    f.store
        .create_task_dependency(
            &f.ctx,
            &f.project.id,
            &TaskDependencyCreate {
                source_task_id: c.id.clone(),
                target_task_id: d.id.clone(),
                kind: TaskDependencyKind::Blocks,
                source_revision: c.revision,
                target_revision: d.revision,
                reason: "D waits for C".into(),
            },
        )
        .unwrap();
    let b = f.store.get_task(&f.ctx, &f.project.id, &b.id).unwrap();
    assert!(
        f.store
            .transition_task(
                &developer,
                &f.project.id,
                &b.id,
                "finish",
                &TaskTransitionRequest {
                    expected_revision: b.revision,
                    ..Default::default()
                }
            )
            .is_err()
    );
    let a = f.store.get_task(&f.ctx, &f.project.id, &a.id).unwrap();
    let c = f.store.get_task(&f.ctx, &f.project.id, &c.id).unwrap();
    let d = f.store.get_task(&f.ctx, &f.project.id, &d.id).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for (source, target) in [(&b, &c), (&d, &a)] {
        let mut store = PlatformStore::connect(&f.config, &f.namespace).unwrap();
        let ctx = f.ctx.clone();
        let project = f.project.id.clone();
        let barrier = barrier.clone();
        let input = TaskDependencyCreate {
            source_task_id: source.id.clone(),
            target_task_id: target.id.clone(),
            kind: TaskDependencyKind::Blocks,
            source_revision: source.revision,
            target_revision: target.revision,
            reason: "Concurrent proposed edge".into(),
        };
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            store.create_task_dependency(&ctx, &project, &input)
        }));
    }
    barrier.wait();
    let results = workers
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "exactly one of two disjoint edges closing a four-task cycle may commit"
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(PlatformError::Validation(_))))
            .count(),
        1
    );
    // Retain cycle assertions, then remove the winning optional edge before execution.
    for edge in results.into_iter().filter_map(Result::ok) {
        f.store
            .remove_task_dependency(
                &f.ctx,
                &f.project.id,
                &edge.id,
                &TaskDependencyChange {
                    expected_revision: edge.revision,
                    reason: "Release concurrency-only requirement".into(),
                },
            )
            .unwrap();
    }
    assert!(
        f.store
            .resolve_task_dependency(
                &f.ctx,
                &f.project.id,
                &ab.id,
                &TaskDependencyChange {
                    expected_revision: ab.revision,
                    reason: "Cannot resolve unfinished prerequisite".into()
                }
            )
            .is_err()
    );
    let a = f.store.get_task(&f.ctx, &f.project.id, &a.id).unwrap();
    let a = f
        .store
        .take_task(
            &developer,
            &a.id,
            &TaskTake {
                expected_revision: a.revision,
                workflow_id: None,
            },
            Some(&f.project.id),
        )
        .unwrap();
    f.store
        .transition_task(
            &developer,
            &f.project.id,
            &a.id,
            "finish",
            &TaskTransitionRequest {
                expected_revision: a.revision,
                ..Default::default()
            },
        )
        .unwrap();
    let resolved = f
        .store
        .resolve_task_dependency(
            &f.ctx,
            &f.project.id,
            &ab.id,
            &TaskDependencyChange {
                expected_revision: ab.revision,
                reason: "Upstream condition verified".into(),
            },
        )
        .unwrap();
    assert_eq!(resolved.status, TaskDependencyStatus::Resolved);
    let b = f.store.get_task(&f.ctx, &f.project.id, &b.id).unwrap();
    let b = f
        .store
        .take_task(
            &developer,
            &b.id,
            &TaskTake {
                expected_revision: b.revision,
                workflow_id: None,
            },
            Some(&f.project.id),
        )
        .unwrap();
    let finished = f
        .store
        .transition_task(
            &developer,
            &f.project.id,
            &b.id,
            "finish",
            &TaskTransitionRequest {
                expected_revision: b.revision,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(finished.state_id.as_deref(), Some("done"));
    let removed = f
        .store
        .remove_task_dependency(
            &f.ctx,
            &f.project.id,
            &ab.id,
            &TaskDependencyChange {
                expected_revision: resolved.revision,
                reason: "Retain trace, remove relation".into(),
            },
        )
        .unwrap();
    assert!(removed.removed_at.is_some());
    let page = f
        .store
        .task_dependency_page(
            &f.ctx,
            &f.project.id,
            &b.id,
            &TaskDependencyQuery {
                include_removed: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(page.items.iter().any(|item| item.id == ab.id));
    assert!(matches!(
        f.store.task_dependency_page(
            &f.ctx,
            "other-project",
            &b.id,
            &TaskDependencyQuery::default()
        ),
        Err(PlatformError::NotFound)
    ));
    f.store
        .create_task_dependency(
            &f.ctx,
            &f.project.id,
            &TaskDependencyCreate {
                source_task_id: left.id.clone(),
                target_task_id: right.id.clone(),
                kind: TaskDependencyKind::DependsOn,
                source_revision: left.revision,
                target_revision: right.revision,
                reason: "Left depends on right".into(),
            },
        )
        .unwrap();
    let left = f.store.get_task(&f.ctx, &f.project.id, &left.id).unwrap();
    let right = f.store.get_task(&f.ctx, &f.project.id, &right.id).unwrap();
    assert!(matches!(
        f.store.create_task_dependency(
            &f.ctx,
            &f.project.id,
            &TaskDependencyCreate {
                source_task_id: left.id.clone(),
                target_task_id: right.id.clone(),
                kind: TaskDependencyKind::Blocks,
                source_revision: left.revision,
                target_revision: right.revision,
                reason: "Opposite normalized edge is cyclic".into()
            }
        ),
        Err(PlatformError::Validation(_))
    ));
    f.store
        .create_task_dependency(
            &f.ctx,
            &f.project.id,
            &TaskDependencyCreate {
                source_task_id: left.id,
                target_task_id: right.id,
                kind: TaskDependencyKind::RelatedTo,
                source_revision: left.revision,
                target_revision: right.revision,
                reason: "Related does not block".into(),
            },
        )
        .unwrap();
}
