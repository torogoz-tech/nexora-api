#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use serde_json::json;
use task_pool_fixture::TaskPoolFixture;
fn finish(f: &mut TaskPoolFixture, id: &str) -> WorkTask {
    let before = f.work.store.pool_task(&f.developer, id).unwrap();
    let taken = f
        .work
        .store
        .take_task(
            &f.developer,
            id,
            &TaskTake {
                expected_revision: before.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    f.work
        .store
        .transition_pool_task(
            &f.developer,
            id,
            "finish",
            &TaskTransitionRequest {
                expected_revision: taken.revision,
                ..Default::default()
            },
        )
        .unwrap()
}
#[test]
#[ignore = "requires PostgreSQL; root pool subtask and prerequisite invariants"]
fn task_hierarchy_starts_before_children_but_completion_requires_all_children() {
    let mut f = TaskPoolFixture::new(false);
    let parent = f.create(None, None);
    let child = f
        .work
        .store
        .create_task_subtask(
            &f.manager,
            &parent.id,
            &TaskSubtaskCreate {
                work_kind: TaskSubtaskKind::Decomposition,
                tested_repository_id: None,
                tested_commit_sha: None,
                expected_revision: parent.revision,
                definition: TaskCreate {
                    title: "Child".into(),
                    origin: "test".into(),
                    type_code: "task".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let relations = f
        .work
        .store
        .task_relations(&f.developer, &parent.id)
        .unwrap();
    assert_eq!(relations["subtasks_total"], 1);
    assert_eq!(relations["can_complete"], false);
    assert_eq!(child.lifecycle, TaskLifecycle::Pending);
    let parent = f.work.store.pool_task(&f.developer, &parent.id).unwrap();
    let parent = f
        .work
        .store
        .take_task(
            &f.developer,
            &parent.id,
            &TaskTake {
                expected_revision: parent.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    assert!(
        f.work
            .store
            .transition_pool_task(
                &f.developer,
                &parent.id,
                "finish",
                &TaskTransitionRequest {
                    expected_revision: parent.revision,
                    ..Default::default()
                }
            )
            .is_err()
    );
    finish(&mut f, &child.id);
    let parent = f
        .work
        .store
        .transition_pool_task(
            &f.developer,
            &parent.id,
            "finish",
            &TaskTransitionRequest {
                expected_revision: parent.revision,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(parent.lifecycle, TaskLifecycle::Completed);
    assert_eq!(
        f.work
            .store
            .task_relations(&f.developer, &parent.id)
            .unwrap()["subtasks_completed"],
        1
    );
}
#[test]
#[ignore = "requires PostgreSQL; completion automatically satisfies blockers"]
fn task_prerequisite_prevents_take_and_unblocks_without_manual_resolution() {
    let mut f = TaskPoolFixture::new(false);
    let a = f.create(None, None);
    let b = f.create(None, None);
    let edge = f
        .work
        .store
        .create_pool_task_dependency(
            &f.manager,
            &a.id,
            &TaskDependencyCreate {
                source_task_id: a.id.clone(),
                target_task_id: b.id.clone(),
                kind: TaskDependencyKind::DependsOn,
                source_revision: a.revision,
                target_revision: b.revision,
                reason: "Do B first".into(),
            },
        )
        .unwrap();
    let a = f.work.store.pool_task(&f.developer, &a.id).unwrap();
    assert!(
        f.work
            .store
            .take_task(
                &f.developer,
                &a.id,
                &TaskTake {
                    expected_revision: a.revision,
                    workflow_id: None
                },
                None
            )
            .is_err()
    );
    assert_eq!(
        f.work.store.task_relations(&f.developer, &a.id).unwrap()["blocking_count"],
        1
    );
    assert!(
        f.work
            .store
            .remove_pool_task_dependency(
                &f.developer,
                &a.id,
                &edge.id,
                &TaskDependencyChange {
                    expected_revision: 0,
                    reason: "bad revision".into()
                }
            )
            .is_err()
    );
    finish(&mut f, &b.id);
    assert_eq!(
        f.work.store.task_relations(&f.developer, &a.id).unwrap()["can_take"],
        true
    );
    let a = f
        .work
        .store
        .take_task(
            &f.developer,
            &a.id,
            &TaskTake {
                expected_revision: a.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    assert_eq!(a.lifecycle, TaskLifecycle::Active);
}
#[test]
#[ignore = "requires PostgreSQL; cycles scopes and completed-parent integrity"]
fn task_relations_reject_cycles_scope_mismatches_and_unfinished_completed_parent() {
    let mut f = TaskPoolFixture::new(false);
    let a = f.create(None, None);
    let b = f.create(None, None);
    let c = f.create(None, None);
    let link = |a: &WorkTask, b: &WorkTask| TaskSubtaskChange {
        expected_revision: a.revision,
        child_revision: b.revision,
        reason: "Decompose".into(),
    };
    f.work
        .store
        .change_task_subtask(&f.manager, &a.id, &b.id, &link(&a, &b), false)
        .unwrap();
    let a = f.work.store.pool_task(&f.manager, &a.id).unwrap();
    let b = f.work.store.pool_task(&f.manager, &b.id).unwrap();
    assert!(
        f.work
            .store
            .change_task_subtask(&f.manager, &b.id, &a.id, &link(&b, &a), false)
            .is_err()
    );
    assert!(
        f.work
            .store
            .change_task_subtask(&f.manager, &c.id, &b.id, &link(&c, &b), false)
            .is_err()
    );
    assert!(
        f.work
            .store
            .create_pool_task_dependency(
                &f.manager,
                &b.id,
                &TaskDependencyCreate {
                    source_task_id: b.id.clone(),
                    target_task_id: a.id.clone(),
                    kind: TaskDependencyKind::DependsOn,
                    source_revision: b.revision,
                    target_revision: a.revision,
                    reason: "would deadlock".into()
                }
            )
            .is_err()
    );
    let project_task = f.create(None, Some(f.work.project.id.clone()));
    assert!(
        f.work
            .store
            .change_task_subtask(
                &f.manager,
                &c.id,
                &project_task.id,
                &link(&c, &project_task),
                false
            )
            .is_err()
    );
    finish(&mut f, &c.id);
    let c = f.work.store.pool_task(&f.manager, &c.id).unwrap();
    let unfinished = f.create(None, None);
    assert!(
        f.work
            .store
            .change_task_subtask(
                &f.manager,
                &c.id,
                &unfinished.id,
                &link(&c, &unfinished),
                false
            )
            .is_err()
    );
    let _ = json!({"retained":a.id});
}
fn sql(f: &TaskPoolFixture) -> postgres::Client {
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(f.work.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut client = postgres::Config::new()
        .host(&f.work.config.host)
        .port(f.work.config.postgres_port)
        .user(&f.work.config.postgres_user)
        .password(password.trim())
        .dbname(&f.work.config.postgres_database)
        .options("-c lock_timeout=5000 -c statement_timeout=10000")
        .connect(postgres::NoTls)
        .unwrap();
    client
        .batch_execute(&format!(
            "SET search_path TO \"{}\",pg_catalog",
            f.work.namespace
        ))
        .unwrap();
    client
}
#[test]
#[ignore = "requires PostgreSQL; released legacy edges remain released"]
fn legacy_resolved_dependencies_do_not_reactivate_or_poison_other_mutations() {
    let mut f = TaskPoolFixture::new(false);
    let a = f.create(None, None);
    let b = f.create(None, None);
    let c = f.create(None, None);
    let mut db = sql(&f);
    db.execute("INSERT INTO task_dependencies(id,project_id,source_task_id,target_task_id,kind,status,created_by) VALUES('legacy_released',NULL,$1,$2,'depends_on','resolved',$3)",&[&a.id,&b.id,&f.manager.actor_id()]).unwrap();
    f.work
        .store
        .create_pool_task_dependency(
            &f.manager,
            &b.id,
            &TaskDependencyCreate {
                source_task_id: b.id.clone(),
                target_task_id: a.id.clone(),
                source_revision: b.revision,
                target_revision: a.revision,
                kind: TaskDependencyKind::DependsOn,
                reason: "Inverse of released historical edge".into(),
            },
        )
        .unwrap();
    assert_eq!(
        f.work.store.task_relations(&f.developer, &a.id).unwrap()["blocking_count"],
        0
    );
    let a = f.work.store.pool_task(&f.developer, &a.id).unwrap();
    f.work
        .store
        .take_task(
            &f.developer,
            &a.id,
            &TaskTake {
                expected_revision: a.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    let d = f.create(None, None);
    f.work
        .store
        .change_task_subtask(
            &f.manager,
            &c.id,
            &d.id,
            &TaskSubtaskChange {
                expected_revision: c.revision,
                child_revision: d.revision,
                reason: "Unrelated hierarchy remains usable".into(),
            },
            false,
        )
        .unwrap();
}
#[test]
#[ignore = "requires PostgreSQL; hidden relations and archived children stay safe"]
fn hidden_children_preserve_authoritative_counts_without_identifiers_and_archived_still_block() {
    let mut f = TaskPoolFixture::new(false);
    let project = f.work.project.id.clone();
    let parent = f.create(Some(f.developer.actor_id().into()), Some(project));
    let child = f
        .work
        .store
        .create_task_subtask(
            &f.manager,
            &parent.id,
            &TaskSubtaskCreate {
                work_kind: TaskSubtaskKind::Decomposition,
                tested_repository_id: None,
                tested_commit_sha: None,
                expected_revision: parent.revision,
                definition: TaskCreate {
                    title: "Private child".into(),
                    origin: "test".into(),
                    type_code: "task".into(),
                    assignee_id: Some(f.other.actor_id().into()),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let relations = f
        .work
        .store
        .task_relations(&f.developer, &parent.id)
        .unwrap();
    assert!(relations["subtasks"].as_array().unwrap().is_empty());
    assert_eq!(relations["unfinished_subtasks"], 1);
    assert_eq!(relations["can_complete"], false);
    assert!(!relations.to_string().contains(&child.id));
    f.work
        .store
        .change_pool_task(
            &f.manager,
            &child.id,
            &TaskChange {
                expected_revision: child.revision,
                archived: Some(true),
                reason: "Archive unfinished child".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        f.work
            .store
            .task_relations(&f.developer, &parent.id)
            .unwrap()["can_complete"],
        false
    );
    let parent = f.work.store.pool_task(&f.developer, &parent.id).unwrap();
    let parent = f
        .work
        .store
        .take_task(
            &f.developer,
            &parent.id,
            &TaskTake {
                expected_revision: parent.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    assert!(
        f.work
            .store
            .transition_pool_task(
                &f.developer,
                &parent.id,
                "finish",
                &TaskTransitionRequest {
                    expected_revision: parent.revision,
                    ..Default::default()
                }
            )
            .is_err()
    );
    let archived = f.work.store.pool_task(&f.manager, &child.id).unwrap();
    f.work
        .store
        .change_task_subtask(
            &f.manager,
            &parent.id,
            &child.id,
            &TaskSubtaskChange {
                expected_revision: parent.revision,
                child_revision: archived.revision,
                reason: "Detach archived incomplete work explicitly".into(),
            },
            true,
        )
        .unwrap();
    let parent = f.work.store.pool_task(&f.developer, &parent.id).unwrap();
    assert!(
        f.work
            .store
            .transition_pool_task(
                &f.developer,
                &parent.id,
                "finish",
                &TaskTransitionRequest {
                    expected_revision: parent.revision,
                    ..Default::default()
                }
            )
            .is_ok()
    );
}
#[test]
#[ignore = "requires PostgreSQL; same actor competing take and graph mutation serializes safely"]
fn same_actor_take_and_dependency_insert_have_no_deadlock_or_partial_execution() {
    use nexora_platform_store::PlatformStore;
    use std::sync::{Arc, Barrier};
    let mut f = TaskPoolFixture::new(false);
    let input = |title: &str| PoolTaskCreate {
        project_id: None,
        definition: TaskCreate {
            title: title.into(),
            origin: "test".into(),
            type_code: "task".into(),
            ..Default::default()
        },
    };
    let a = f
        .work
        .store
        .create_pool_task(&f.developer, &input("Concurrent target"))
        .unwrap();
    let b = f
        .work
        .store
        .create_pool_task(&f.developer, &input("Concurrent prerequisite"))
        .unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for taking in [true, false] {
        let ctx = f.developer.clone();
        let config = f.work.config.clone();
        let namespace = f.work.namespace.clone();
        let a = a.clone();
        let b = b.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let mut store = PlatformStore::connect(&config, &namespace).unwrap();
            barrier.wait();
            if taking {
                store
                    .take_task(
                        &ctx,
                        &a.id,
                        &TaskTake {
                            expected_revision: a.revision,
                            workflow_id: None,
                        },
                        None,
                    )
                    .map(|_| ())
            } else {
                store
                    .create_pool_task_dependency(
                        &ctx,
                        &a.id,
                        &TaskDependencyCreate {
                            source_task_id: a.id.clone(),
                            target_task_id: b.id,
                            source_revision: a.revision,
                            target_revision: b.revision,
                            kind: TaskDependencyKind::DependsOn,
                            reason: "Concurrent relation".into(),
                        },
                    )
                    .map(|_| ())
            }
        }));
    }
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .all(|e| matches!(e, PlatformError::Conflict(_))),
        "{results:?}"
    );
    let live = f.work.store.pool_task(&f.developer, &a.id).unwrap();
    let relations = f.work.store.task_relations(&f.developer, &a.id).unwrap();
    assert!(
        live.lifecycle == TaskLifecycle::Active && relations["blocking_count"] == 0
            || live.lifecycle == TaskLifecycle::Pending && relations["blocking_count"] == 1
    );
}
#[test]
#[ignore = "requires PostgreSQL; direct terminal and reopening invariants"]
fn database_guard_rejects_reopening_child_below_completed_parent() {
    let mut f = TaskPoolFixture::new(false);
    let parent = f.create(None, None);
    let child = f
        .work
        .store
        .create_task_subtask(
            &f.manager,
            &parent.id,
            &TaskSubtaskCreate {
                work_kind: TaskSubtaskKind::Decomposition,
                tested_repository_id: None,
                tested_commit_sha: None,
                expected_revision: parent.revision,
                definition: TaskCreate {
                    title: "Child".into(),
                    origin: "test".into(),
                    type_code: "task".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    finish(&mut f, &child.id);
    finish(&mut f, &parent.id);
    let mut db = sql(&f);
    assert!(
        db.execute(
            "UPDATE tasks SET state_id='ready' WHERE id=$1",
            &[&child.id]
        )
        .is_err()
    );
    let unfinished = f.create(None, None);
    let parent = f.work.store.pool_task(&f.manager, &parent.id).unwrap();
    assert!(
        f.work
            .store
            .change_task_subtask(
                &f.manager,
                &parent.id,
                &unfinished.id,
                &TaskSubtaskChange {
                    expected_revision: parent.revision,
                    child_revision: unfinished.revision,
                    reason: "Cannot hide unfinished work".into()
                },
                false
            )
            .is_err()
    );
}
#[test]
#[ignore = "requires PostgreSQL; parent finish versus child link is atomic"]
fn parent_completion_competing_with_subtask_link_preserves_invariant() {
    use nexora_platform_store::PlatformStore;
    use std::sync::{Arc, Barrier};
    let mut f = TaskPoolFixture::new(false);
    let parent = f.create(None, None);
    let child = f.create(None, None);
    let parent = f
        .work
        .store
        .take_task(
            &f.developer,
            &parent.id,
            &TaskTake {
                expected_revision: parent.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    // Developer owns parent; manager owns both endpoints and links child concurrently.
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for completing in [true, false] {
        let ctx = if completing {
            f.developer.clone()
        } else {
            f.manager.clone()
        };
        let config = f.work.config.clone();
        let namespace = f.work.namespace.clone();
        let parent = parent.clone();
        let child = child.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let mut store = PlatformStore::connect(&config, &namespace).unwrap();
            barrier.wait();
            if completing {
                store
                    .transition_pool_task(
                        &ctx,
                        &parent.id,
                        "finish",
                        &TaskTransitionRequest {
                            expected_revision: parent.revision,
                            ..Default::default()
                        },
                    )
                    .map(|_| ())
            } else {
                store
                    .change_task_subtask(
                        &ctx,
                        &parent.id,
                        &child.id,
                        &TaskSubtaskChange {
                            expected_revision: parent.revision,
                            child_revision: child.revision,
                            reason: "Concurrent decomposition".into(),
                        },
                        false,
                    )
                    .map(|_| ())
            }
        }));
    }
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .all(|e| matches!(e, PlatformError::Conflict(_))),
        "{results:?}"
    );
    let live = f.work.store.pool_task(&f.manager, &parent.id).unwrap();
    let relations = f.work.store.task_relations(&f.manager, &parent.id).unwrap();
    assert!(
        live.lifecycle == TaskLifecycle::Completed && relations["subtasks_total"] == 0
            || live.lifecycle == TaskLifecycle::Active && relations["unfinished_subtasks"] == 1
    );
}
#[test]
#[ignore = "requires PostgreSQL; take into terminal initial state respects child requirements"]
fn terminal_initial_take_cannot_complete_parent_before_children() {
    let mut f = TaskPoolFixture::new(false);
    let parent = f.create(None, None);
    let child = f
        .work
        .store
        .create_task_subtask(
            &f.manager,
            &parent.id,
            &TaskSubtaskCreate {
                work_kind: TaskSubtaskKind::Decomposition,
                tested_repository_id: None,
                tested_commit_sha: None,
                expected_revision: parent.revision,
                definition: TaskCreate {
                    title: "Child".into(),
                    origin: "test".into(),
                    type_code: "task".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let flow=f.work.store.create_developer_workflow(&f.work.ctx,&serde_json::from_value(json!({"name":"Immediate completion","graph":{"states":[{"id":"done","name":"Done","initial":true,"terminal":true}],"transitions":[]}})).unwrap()).unwrap();
    let version = f
        .work
        .store
        .publish_developer_workflow(
            &f.work.ctx,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Test immediate completion".into(),
            },
        )
        .unwrap();
    f.work
        .store
        .assign_actor_workflow(
            &f.work.ctx,
            f.developer.actor_id(),
            &DeveloperWorkflowChange {
                expected_revision: 1,
                workflow_version_id: version.id,
                reason: "Completion workflow".into(),
            },
        )
        .unwrap();
    let parent = f.work.store.pool_task(&f.developer, &parent.id).unwrap();
    assert!(
        f.work
            .store
            .take_task(
                &f.developer,
                &parent.id,
                &TaskTake {
                    expected_revision: parent.revision,
                    workflow_id: None
                },
                None
            )
            .is_err()
    );
    let child = f
        .work
        .store
        .take_task(
            &f.developer,
            &child.id,
            &TaskTake {
                expected_revision: child.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    assert_eq!(child.lifecycle, TaskLifecycle::Completed);
    let parent = f
        .work
        .store
        .take_task(
            &f.developer,
            &parent.id,
            &TaskTake {
                expected_revision: parent.revision,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    assert_eq!(parent.lifecycle, TaskLifecycle::Completed);
}
