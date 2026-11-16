#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use nexora_platform_store::{PlatformStore, SecretGenerator};
use std::sync::{Arc, Barrier};
use task_pool_fixture::TaskPoolFixture;

// These regressions exercise historical rows; new governed behavior has separate coverage.
fn legacy_task(
    assignee: Option<String>,
    project: Option<String>,
    f: &mut TaskPoolFixture,
) -> WorkTask {
    let task = f.create(assignee, project);
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(f.work.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&f.work.config.host)
        .port(f.work.config.postgres_port)
        .user(&f.work.config.postgres_user)
        .password(password.trim())
        .dbname(&f.work.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{}\"", f.work.namespace))
        .unwrap();
    sql.execute(
        "UPDATE tasks SET governed_stage=false WHERE id=$1",
        &[&task.id],
    )
    .unwrap();
    f.work.store.pool_task(&f.work.ctx, &task.id).unwrap()
}

#[test]
#[ignore = "requires PostgreSQL; explicit pending/reserved/taken/completed lifecycle"]
fn reservation_does_not_bind_workflow_and_terminal_keeps_pool_history() {
    let mut f = TaskPoolFixture::new(false);
    // Manager has no published execution workflow and can create/reserve tasks.
    let mut task = legacy_task(Some(f.manager.actor_id().into()), None, &mut f);
    assert_eq!(task.lifecycle, TaskLifecycle::Pending);
    assert!(task.project_id.is_none() && task.workflow_id.is_none() && task.state_id.is_none());
    assert!(
        f.work
            .store
            .take_task(
                &f.developer,
                &task.id,
                &TaskTake {
                    expected_revision: task.revision,
                    workflow_id: None
                },
                None
            )
            .is_err()
    );
    task = f
        .work
        .store
        .change_pool_task(
            &f.manager,
            &task.id,
            &TaskChange {
                expected_revision: task.revision,
                assignee_id: Some(f.developer.actor_id().into()),
                reason: "Reserve execution".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(task.lifecycle, TaskLifecycle::Pending);
    assert!(
        f.work
            .store
            .developer_board_tasks(&f.developer, None, 100)
            .unwrap()
            .items
            .is_empty()
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
    assert_eq!(task.lifecycle, TaskLifecycle::Active);
    assert_eq!(
        task.workflow_version_id.as_deref(),
        Some(f.version.id.as_str())
    );
    assert_eq!(
        f.work
            .store
            .developer_board_tasks(&f.developer, None, 100)
            .unwrap()
            .items
            .len(),
        1
    );
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
    assert!(
        f.work
            .store
            .developer_board_tasks(&f.developer, None, 100)
            .unwrap()
            .items
            .is_empty()
    );
    let page = f
        .work
        .store
        .visible_tasks(&f.developer, &VisibleTaskQuery::default())
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].task.lifecycle, TaskLifecycle::Completed);
    assert_eq!(
        f.work
            .store
            .pool_task_history(&f.developer, &task.id, None, 100)
            .unwrap()
            .items
            .len(),
        4
    );
    let comment = f
        .work
        .store
        .add_pool_task_comment(
            &f.other,
            &task.id,
            &TaskCommentCreate {
                body: "Retained team discussion".into(),
            },
        )
        .unwrap();
    assert!(comment.project_id.is_none());
    assert_eq!(
        f.work
            .store
            .pool_task_comments(&f.manager, &task.id, None, 100)
            .unwrap()
            .items
            .len(),
        1
    );
}

#[test]
#[ignore = "requires PostgreSQL; reassignment resets execution without changing published graphs"]
fn active_reassignment_returns_pending_and_completed_assignment_does_not_reopen() {
    let mut f = TaskPoolFixture::new(false);
    let task = legacy_task(None, None, &mut f);
    let task = f
        .work
        .store
        .take_task(
            &f.developer,
            &task.id,
            &TaskTake {
                expected_revision: 1,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    let task = f
        .work
        .store
        .change_pool_task(
            &f.manager,
            &task.id,
            &TaskChange {
                expected_revision: task.revision,
                assignee_id: Some(f.other.actor_id().into()),
                reason: "Handoff".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(task.lifecycle, TaskLifecycle::Pending);
    assert!(task.workflow_id.is_none());
    assert!(
        f.work
            .store
            .developer_board_tasks(&f.other, None, 100)
            .unwrap()
            .items
            .is_empty()
    );
    let task = f
        .work
        .store
        .take_task(
            &f.other,
            &task.id,
            &TaskTake {
                expected_revision: task.revision,
                workflow_id: Some(f.version.workflow_id.clone()),
            },
            None,
        )
        .unwrap();
    let task = f
        .work
        .store
        .transition_pool_task(
            &f.other,
            &task.id,
            "finish",
            &TaskTransitionRequest {
                expected_revision: task.revision,
                ..Default::default()
            },
        )
        .unwrap();
    let task = f
        .work
        .store
        .change_pool_task(
            &f.manager,
            &task.id,
            &TaskChange {
                expected_revision: task.revision,
                clear_assignee: true,
                reason: "Close ownership".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(task.lifecycle, TaskLifecycle::Completed);
    assert!(task.workflow_id.is_some());
    assert_eq!(
        f.work
            .store
            .developer_workflow_version(&f.work.ctx, &f.version.workflow_id, &f.version.id)
            .unwrap()
            .checksum,
        f.version.checksum
    );
}

#[test]
#[ignore = "requires PostgreSQL; concurrent takes and same actor lock ordering"]
fn concurrent_take_has_exactly_one_winner() {
    let mut f = TaskPoolFixture::new(false);
    let task = legacy_task(None, None, &mut f);
    let barrier = Arc::new(Barrier::new(2));
    let mut threads = Vec::new();
    for ctx in [f.developer.clone(), f.other.clone()] {
        let config = f.work.config.clone();
        let namespace = f.work.namespace.clone();
        let id = task.id.clone();
        let barrier = barrier.clone();
        let workflow_id = f.version.workflow_id.clone();
        threads.push(std::thread::spawn(move || {
            let mut store = PlatformStore::connect(&config, &namespace).unwrap();
            barrier.wait();
            store.take_task(
                &ctx,
                &id,
                &TaskTake {
                    expected_revision: 1,
                    workflow_id: Some(workflow_id),
                },
                None,
            )
        }));
    }
    let outcomes: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(
        outcomes
            .iter()
            .filter_map(|r| r.as_ref().err())
            .all(|e| matches!(e, PlatformError::Conflict(_)))
    );
    let one = legacy_task(None, None, &mut f);
    let two = legacy_task(None, None, &mut f);
    let barrier = Arc::new(Barrier::new(2));
    let mut threads = Vec::new();
    for id in [one.id, two.id] {
        let config = f.work.config.clone();
        let namespace = f.work.namespace.clone();
        let ctx = f.developer.clone();
        let barrier = barrier.clone();
        let workflow_id = f.version.workflow_id.clone();
        threads.push(std::thread::spawn(move || {
            let mut store = PlatformStore::connect(&config, &namespace).unwrap();
            barrier.wait();
            store.take_task(
                &ctx,
                &id,
                &TaskTake {
                    expected_revision: 1,
                    workflow_id: Some(workflow_id),
                },
                None,
            )
        }));
    }
    for thread in threads {
        assert!(thread.join().unwrap().is_ok());
    }
}

#[test]
#[ignore = "requires PostgreSQL; projectless reads, write authority and key attenuation"]
fn global_visibility_does_not_allow_hijacking_and_keys_retain_scope() {
    let mut f = TaskPoolFixture::new(false);
    let task = legacy_task(Some(f.developer.actor_id().into()), None, &mut f);
    // A third ordinary developer can see all projectless tasks, but cannot hijack reservations.
    let pass = zeroize::Zeroizing::new(SecretGenerator::token("password").unwrap());
    let actor = f
        .work
        .store
        .create_user(&f.work.ctx, "observer", &pass, "Observer")
        .unwrap();
    f.work
        .store
        .change_member_role(
            &f.work.ctx,
            &actor.id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![f.work.project.id.clone()],
                expected_revision: 0,
            },
        )
        .unwrap();
    let session = f.work.store.login("observer", &pass).unwrap();
    let gate = f
        .work
        .store
        .identity_gate(&session.access_token, "skip", "")
        .unwrap();
    let observer = f
        .work
        .store
        .authenticate(gate["access_token"].as_str().unwrap())
        .unwrap();
    assert!(f.work.store.pool_task(&observer, &task.id).is_ok());
    assert!(
        f.work
            .store
            .change_pool_task(
                &observer,
                &task.id,
                &TaskChange {
                    expected_revision: 1,
                    assignee_id: Some(observer.actor_id().into()),
                    reason: "Steal".into(),
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert!(
        f.work
            .store
            .take_task(
                &observer,
                &task.id,
                &TaskTake {
                    expected_revision: 1,
                    workflow_id: None
                },
                None
            )
            .is_err()
    );
    for installation in [false, true] {
        if installation {
            f.work
                .store
                .change_member_role(
                    &f.work.ctx,
                    f.developer.actor_id(),
                    &MemberRoleChange {
                        role: MemberRole::Developer,
                        all_projects: true,
                        project_ids: vec![],
                        expected_revision: 1,
                    },
                )
                .unwrap();
        }
        let key = f
            .work
            .store
            .issue_key(
                &f.work.ctx,
                f.developer.actor_id(),
                "Scoped pool key",
                KeyScope {
                    capabilities: vec![
                        Capability::TasksRead,
                        Capability::TasksWrite,
                        Capability::TasksTransition,
                    ],
                    project_ids: if installation {
                        vec![]
                    } else {
                        vec![f.work.project.id.clone()]
                    },
                    all_projects: false,
                    installation,
                },
                3600,
            )
            .unwrap();
        let api = f.work.store.authenticate(&key.key).unwrap();
        let page = f
            .work
            .store
            .visible_tasks(&api, &VisibleTaskQuery::default())
            .unwrap();
        assert_eq!(page.projectless_tasks_included, installation);
        if installation {
            assert!(f.work.store.pool_task(&api, &task.id).is_ok());
            assert!(
                f.work
                    .store
                    .take_task(
                        &api,
                        &task.id,
                        &TaskTake {
                            expected_revision: 1,
                            workflow_id: None
                        },
                        None
                    )
                    .is_ok()
            );
            let active = f
                .work
                .store
                .visible_tasks(
                    &api,
                    &VisibleTaskQuery {
                        lifecycle: Some(TaskLifecycle::Active),
                        ..Default::default()
                    },
                )
                .unwrap();
            assert!(active.projectless_tasks_included);
            assert!(active.items.iter().any(|item| item.task.id == task.id));
        } else {
            assert_eq!(
                f.work
                    .store
                    .visible_tasks(&api, &VisibleTaskQuery::default())
                    .unwrap()
                    .total,
                0
            );
            assert!(f.work.store.pool_task(&api, &task.id).is_err());
            assert!(
                f.work
                    .store
                    .take_task(
                        &api,
                        &task.id,
                        &TaskTake {
                            expected_revision: 1,
                            workflow_id: None
                        },
                        None
                    )
                    .is_err()
            );
        }
    }
    let pending = legacy_task(None, None, &mut f);
    assert!(
        f.work
            .store
            .change_pool_task(
                &observer,
                &pending.id,
                &TaskChange {
                    expected_revision: 1,
                    assignee_id: Some(observer.actor_id().into()),
                    reason: "Reserve".into(),
                    ..Default::default()
                }
            )
            .is_ok()
    );
}

#[test]
#[ignore = "requires PostgreSQL; projectless workflow approvals remain human-only and scoped"]
fn projectless_human_approval_is_bound_to_exact_revision_and_consumed_atomically() {
    let mut f = TaskPoolFixture::new(true);
    let task = legacy_task(None, None, &mut f);
    let task = f
        .work
        .store
        .take_task(
            &f.developer,
            &task.id,
            &TaskTake {
                expected_revision: 1,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    let parameters = TaskTransitionRequest {
        expected_revision: task.revision,
        ..Default::default()
    };
    assert!(
        f.work
            .store
            .transition_pool_task(&f.developer, &task.id, "finish", &parameters)
            .is_err()
    );
    let grant = TaskApprovalGrant {
        transition_id: "finish".into(),
        parameters: parameters.clone(),
        reason: "Reviewed".into(),
        expires_in_seconds: 3600,
    };
    assert!(
        f.work
            .store
            .grant_pool_task_approval(&f.developer, &task.id, &grant)
            .is_err()
    );
    let approval = f
        .work
        .store
        .grant_pool_task_approval(&f.manager, &task.id, &grant)
        .unwrap();
    assert!(approval.project_id.is_none());
    let completed = f
        .work
        .store
        .transition_pool_task(
            &f.developer,
            &task.id,
            "finish",
            &TaskTransitionRequest {
                approval_id: Some(approval.id.clone()),
                ..parameters.clone()
            },
        )
        .unwrap();
    assert_eq!(completed.lifecycle, TaskLifecycle::Completed);
    assert!(
        f.work
            .store
            .get_pool_task_approval(&f.manager, &task.id, &approval.id)
            .unwrap()
            .consumed_at
            .is_some()
    );
    assert!(
        f.work
            .store
            .transition_pool_task(
                &f.developer,
                &task.id,
                "finish",
                &TaskTransitionRequest {
                    approval_id: Some(approval.id),
                    ..parameters
                }
            )
            .is_err()
    );
}

#[test]
#[ignore = "requires PostgreSQL; recognized roles, safe options and project privacy"]
fn all_human_roles_can_create_projectless_without_exposing_project_work() {
    let mut f = TaskPoolFixture::new(false);
    let password = zeroize::Zeroizing::new(SecretGenerator::token("password").unwrap());
    let actor = f
        .work
        .store
        .create_user(&f.work.ctx, "pmo", &password, "PMO")
        .unwrap();
    f.work
        .store
        .change_member_role(
            &f.work.ctx,
            &actor.id,
            &MemberRoleChange {
                role: MemberRole::Pmo,
                all_projects: false,
                project_ids: vec![f.work.project.id.clone()],
                expected_revision: 0,
            },
        )
        .unwrap();
    let session = f.work.store.login("pmo", &password).unwrap();
    let gate = f
        .work
        .store
        .identity_gate(&session.access_token, "skip", "")
        .unwrap();
    let pmo = f
        .work
        .store
        .authenticate(gate["access_token"].as_str().unwrap())
        .unwrap();
    for ctx in [
        f.work.ctx.clone(),
        f.developer.clone(),
        f.other.clone(),
        f.manager.clone(),
        pmo,
    ] {
        let task = f
            .work
            .store
            .create_pool_task(
                &ctx,
                &PoolTaskCreate {
                    project_id: None,
                    definition: TaskCreate {
                        title: "Team intake".into(),
                        type_code: "task".into(),
                        origin: "test".into(),
                        ..Default::default()
                    },
                },
            )
            .unwrap();
        assert_eq!(task.lifecycle, TaskLifecycle::Pending);
        assert!(task.assignee_id.is_none());
        let options = f.work.store.task_pool_options(&ctx, None).unwrap();
        assert!(!options["types"].as_array().unwrap().is_empty());
        assert!(
            options["assignees"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["actor_id"] == f.manager.actor_id())
        );
    }
    let scoped = legacy_task(None, Some(f.work.project.id.clone()), &mut f);
    assert!(matches!(
        f.work.store.pool_task(&f.developer, &scoped.id),
        Err(PlatformError::NotFound)
    ));
    assert_eq!(
        f.work
            .store
            .visible_tasks(&f.developer, &VisibleTaskQuery::default())
            .unwrap()
            .total,
        5
    );
    let grantless = f
        .work
        .store
        .create_user(&f.work.ctx, "grantless", &password, "Pending invitation")
        .unwrap();
    let session = f.work.store.login("grantless", &password).unwrap();
    let gate = f
        .work
        .store
        .identity_gate(&session.access_token, "skip", "")
        .unwrap();
    let grantless_ctx = f
        .work
        .store
        .authenticate(gate["access_token"].as_str().unwrap())
        .unwrap();
    assert!(
        f.work
            .store
            .create_pool_task(
                &grantless_ctx,
                &PoolTaskCreate {
                    project_id: None,
                    definition: TaskCreate {
                        title: "No authority".into(),
                        type_code: "task".into(),
                        origin: "test".into(),
                        ..Default::default()
                    }
                }
            )
            .is_err()
    );
    assert!(
        !f.work.store.task_pool_options(&f.developer, None).unwrap()["assignees"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["actor_id"] == grantless.id)
    );
}

#[test]
#[ignore = "requires PostgreSQL; exact take retries, changed fingerprints and revoked keys"]
fn take_receipts_are_exact_and_reauthorize_live_credentials() {
    let mut f = TaskPoolFixture::new(false);
    f.work
        .store
        .change_member_role(
            &f.work.ctx,
            f.developer.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: true,
                project_ids: vec![],
                expected_revision: 1,
            },
        )
        .unwrap();
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.developer.actor_id(),
            "Global execution key",
            KeyScope {
                capabilities: vec![Capability::TasksRead, Capability::TasksTransition],
                project_ids: vec![],
                all_projects: false,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let api = f.work.store.authenticate(&key.key).unwrap();
    let task = legacy_task(None, None, &mut f);
    let input = TaskTake {
        expected_revision: 1,
        workflow_id: None,
    };
    let canonical = serde_json::json!({"task_id":task.id,"take":input});
    let taken = f
        .work
        .store
        .request(&api, "task.take", "take-once", &canonical, |store| {
            store.take_task(&api, &task.id, &input, None)
        })
        .unwrap();
    let retry = f
        .work
        .store
        .request(&api, "task.take", "take-once", &canonical, |store| {
            store.take_task(&api, &task.id, &input, None)
        })
        .unwrap();
    assert_eq!(
        serde_json::to_value(&taken).unwrap(),
        serde_json::to_value(retry).unwrap()
    );
    assert_eq!(
        f.work
            .store
            .pool_task_history(&f.manager, &task.id, None, 100)
            .unwrap()
            .items
            .len(),
        2
    );
    let changed = serde_json::json!({"task_id":task.id,"take":{"expected_revision":2}});
    assert!(matches!(
        f.work
            .store
            .request(&api, "task.take", "take-once", &changed, |store| store
                .take_task(
                    &api,
                    &task.id,
                    &TaskTake {
                        expected_revision: 2,
                        workflow_id: None
                    },
                    None
                )),
        Err(PlatformError::Conflict(_))
    ));
    f.work
        .store
        .revoke_key(&f.work.ctx, &key.metadata.id)
        .unwrap();
    assert!(
        f.work
            .store
            .request(&api, "task.take", "take-once", &canonical, |store| store
                .take_task(&api, &task.id, &input, None))
            .is_err()
    );
}

#[test]
#[ignore = "requires PostgreSQL; taking a terminal initial state completes immediately"]
fn terminal_initial_state_completes_without_entering_kanban() {
    let mut f = TaskPoolFixture::new(false);
    let flow=f.work.store.create_developer_workflow(&f.work.ctx,&serde_json::from_value(serde_json::json!({"name":"Immediate completion","graph":{"states":[{"id":"done","name":"Done","initial":true,"terminal":true}],"transitions":[]}})).unwrap()).unwrap();
    let version = f
        .work
        .store
        .publish_developer_workflow(
            &f.work.ctx,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Single-state completion".into(),
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
                reason: "Use completion policy".into(),
            },
        )
        .unwrap();
    let task = legacy_task(None, None, &mut f);
    let completed = f
        .work
        .store
        .take_task(
            &f.developer,
            &task.id,
            &TaskTake {
                expected_revision: 1,
                workflow_id: None,
            },
            None,
        )
        .unwrap();
    assert_eq!(completed.lifecycle, TaskLifecycle::Completed);
    assert!(
        f.work
            .store
            .developer_board_tasks(&f.developer, None, 100)
            .unwrap()
            .items
            .is_empty()
    );
    f.work
        .store
        .remove_actor_workflow(
            &f.work.ctx,
            f.developer.actor_id(),
            &DeveloperWorkflowRemoval {
                expected_revision: 2,
                reason: "Completed history does not block membership".into(),
            },
        )
        .unwrap();
    assert_eq!(
        f.work
            .store
            .pool_task(&f.developer, &task.id)
            .unwrap()
            .lifecycle,
        TaskLifecycle::Completed
    );
}
