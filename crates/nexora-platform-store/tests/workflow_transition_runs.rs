#[path = "../../nexora-source/tests/support/git_fixture.rs"]
mod git_fixture;
#[path = "support/gitlab_source_fixture.rs"]
mod gitlab_source_fixture;
#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
#[path = "../../nexora-source/tests/support/test_directory.rs"]
mod test_directory;
use nexora_platform::*;
use nexora_platform_store::GitNetworkPolicy;
use serde_json::{Value, json};
use task_pool_fixture::TaskPoolFixture;
fn setup(steps: Value) -> (TaskPoolFixture, WorkTask) {
    let mut f = TaskPoolFixture::new(false);
    let flow=f.work.store.create_developer_workflow(&f.work.ctx,&serde_json::from_value(json!({"name":"Ordered execution","graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","from":"ready","to":"done","steps":steps}]}})).unwrap()).unwrap();
    let version = f
        .work
        .store
        .publish_developer_workflow(
            &f.work.ctx,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Publish rules".into(),
            },
        )
        .unwrap();
    let assigned = f
        .work
        .store
        .actor_workflow(&f.work.ctx, f.developer.actor_id())
        .unwrap();
    f.work
        .store
        .assign_actor_workflow(
            &f.work.ctx,
            f.developer.actor_id(),
            &DeveloperWorkflowChange {
                expected_revision: assigned.unwrap().revision,
                workflow_version_id: version.id,
                reason: "Use ordered execution".into(),
            },
        )
        .unwrap();
    let task = f.create(
        Some(f.developer.actor_id().into()),
        Some(f.work.project.id.clone()),
    );
    // These pre-existing regressions model migrated workflow tasks, whose historical
    // completion contract remains distinct from newly governed stages below.
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
    sql.batch_execute(&format!("SET search_path TO {}", f.work.namespace))
        .unwrap();
    sql.execute(
        "UPDATE tasks SET governed_stage=false WHERE id=$1",
        &[&task.id],
    )
    .unwrap();
    let task = f
        .work
        .store
        .take_task(
            &f.developer,
            &task.id,
            &TaskTake {
                expected_revision: task.revision,
                workflow_id: Some(flow.id),
            },
            None,
        )
        .unwrap();
    (f, task)
}
#[test]
#[ignore = "requires local PostgreSQL; durable approval continuation and safe replay"]
fn approval_request_notifies_human_and_worker_resumes_exact_run_once() {
    let (mut f, task) = setup(
        json!([{"kind":"require_approval"},{"kind":"advance_state"},{"kind":"notify","channel":"in_app"}]),
    );
    let input = TaskTransitionRequest {
        expected_revision: task.revision,
        ..Default::default()
    };
    assert!(
        f.work
            .store
            .transition_pool_task(&f.developer, &task.id, "finish", &input)
            .is_err()
    );
    let runs = f
        .work
        .store
        .task_transition_runs(&f.developer, &task.id)
        .unwrap();
    let run = &runs["items"][0];
    assert_eq!(run["status"], "waiting_approval");
    assert_eq!(run["error_code"], "human_approval_required");
    assert_eq!(
        f.work
            .store
            .pool_task(&f.developer, &task.id)
            .unwrap()
            .revision,
        task.revision
    );
    let request_id = run["approval_request_id"].as_str().unwrap();
    let request = f
        .work
        .store
        .get_approval_request(&f.work.ctx, &f.work.project.id, request_id)
        .unwrap();
    f.work
        .store
        .decide_approval_request(
            &f.work.ctx,
            &f.work.project.id,
            request_id,
            &ApprovalRequestDecide {
                expected_revision: request.revision,
                decision: ApprovalDecision::Approved,
                reason: "Reviewed exact operation".into(),
                approval_expires_in_seconds: 600,
            },
        )
        .unwrap();
    assert_eq!(
        f.work
            .store
            .task_transition_runs(&f.developer, &task.id)
            .unwrap()["items"][0]["status"],
        "ready"
    );
    assert!(
        f.work
            .store
            .resume_one_task_transition(&GitNetworkPolicy::default())
            .unwrap()
    );
    let completed = f.work.store.pool_task(&f.developer, &task.id).unwrap();
    assert_eq!(completed.lifecycle, TaskLifecycle::Completed);
    assert_eq!(completed.revision, task.revision + 1);
    let after = f
        .work
        .store
        .task_transition_runs(&f.developer, &task.id)
        .unwrap();
    assert_eq!(after["items"][0]["status"], "succeeded");
    let replay = f
        .work
        .store
        .retry_task_transition(
            &f.developer,
            &task.id,
            run["id"].as_str().unwrap(),
            &GitNetworkPolicy::default(),
        )
        .unwrap();
    assert_eq!(replay.revision, completed.revision);
    assert!(
        !f.work
            .store
            .resume_one_task_transition(&GitNetworkPolicy::default())
            .unwrap()
    );
    let timeline = f
        .work
        .store
        .task_workflow_timeline(&f.developer, &task.id, None)
        .unwrap();
    assert!(
        timeline["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["operation"] == "task.transition_waiting_approval")
    );
}
#[test]
#[ignore = "requires local PostgreSQL; stale failures timeline and request receipt consistency"]
fn stale_attempt_is_durable_and_idempotency_key_cannot_change_operation() {
    let (mut f, task) = setup(json!([{"kind":"advance_state"}]));
    let stale = TaskTransitionRequest {
        expected_revision: task.revision - 1,
        ..Default::default()
    };
    assert!(
        f.work
            .store
            .transition_pool_task(&f.developer, &task.id, "finish", &stale)
            .is_err()
    );
    let runs = f
        .work
        .store
        .task_transition_runs(&f.developer, &task.id)
        .unwrap();
    assert_eq!(runs["items"][0]["error_code"], "task_revision_changed");
    let input = TaskTransitionRequest {
        expected_revision: task.revision,
        ..Default::default()
    };
    let result = f
        .work
        .store
        .request(
            &f.developer,
            "task.transition",
            "stable-transition",
            &json!(input),
            |store| store.transition_pool_task(&f.developer, &task.id, "finish", &input),
        )
        .unwrap();
    let replay = f
        .work
        .store
        .request(
            &f.developer,
            "task.transition",
            "stable-transition",
            &json!(input),
            |store| store.transition_pool_task(&f.developer, &task.id, "finish", &input),
        )
        .unwrap();
    assert_eq!(result.revision, replay.revision);
    let mut changed = input.clone();
    changed.reason = Some("Changed payload".into());
    assert!(matches!(
        f.work.store.request(
            &f.developer,
            "task.transition",
            "stable-transition",
            &json!(changed),
            |store| store.transition_pool_task(&f.developer, &task.id, "finish", &changed)
        ),
        Err(PlatformError::Conflict(_))
    ));
    let timeline = f
        .work
        .store
        .task_workflow_timeline(&f.developer, &task.id, None)
        .unwrap();
    assert!(
        timeline["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["details"]["code"] == "task_revision_changed")
    );
}
#[test]
#[ignore = "requires local PostgreSQL; notification failure cannot revert or advance twice"]
fn smtp_failure_is_visible_and_retry_requeues_after_atomic_advance() {
    let (mut f, task) =
        setup(json!([{"kind":"advance_state"},{"kind":"notify","channel":"email"}]));
    f.work
        .store
        .change_notification_recipient(
            &f.work.ctx,
            f.developer.actor_id(),
            &NotificationRecipientChange {
                email: "workflow@example.invalid".into(),
                assignments: true,
                approvals: true,
                expected_revision: 0,
            },
        )
        .unwrap();
    let input = TaskTransitionRequest {
        expected_revision: task.revision,
        ..Default::default()
    };
    let result = f
        .work
        .store
        .transition_pool_task(&f.developer, &task.id, "finish", &input)
        .unwrap();
    let runs = f
        .work
        .store
        .task_transition_runs(&f.developer, &task.id)
        .unwrap();
    let run = &runs["items"][0];
    let key = run["notifications"][0]["event_key"]
        .as_str()
        .unwrap()
        .to_owned();
    // Simulate exhausted local delivery attempts without contacting SMTP.
    for _ in 0..5 {
        let mail = f
            .work
            .store
            .claim_email_notification(true)
            .unwrap()
            .unwrap();
        assert_eq!(mail.event_key, key);
        f.work
            .store
            .finish_email_notification(&key, mail.attempts, "pending")
            .unwrap();
        if mail.attempts < 5 {
            let c = &f.work.config;
            let password = zeroize::Zeroizing::new(
                std::fs::read_to_string(c.secrets_directory.join("postgres_password")).unwrap(),
            );
            let mut client = postgres::Config::new()
                .host(&c.host)
                .port(c.postgres_port)
                .user(&c.postgres_user)
                .password(password.trim())
                .dbname(&c.postgres_database)
                .connect(postgres::NoTls)
                .unwrap();
            client
                .batch_execute(&format!("SET search_path TO {}", f.work.namespace))
                .unwrap();
            client.execute("UPDATE email_notifications SET available_at=clock_timestamp() WHERE event_key=$1",&[&key]).unwrap();
        }
    }
    let failed = f
        .work
        .store
        .task_transition_runs(&f.developer, &task.id)
        .unwrap();
    assert_eq!(
        failed["items"][0]["error_code"],
        "notification_delivery_failed"
    );
    assert_eq!(
        f.work
            .store
            .pool_task(&f.developer, &task.id)
            .unwrap()
            .revision,
        result.revision
    );
    let retried = f
        .work
        .store
        .retry_task_transition(
            &f.developer,
            &task.id,
            run["id"].as_str().unwrap(),
            &GitNetworkPolicy::default(),
        )
        .unwrap();
    assert_eq!(retried.revision, result.revision);
    let mail = f
        .work
        .store
        .claim_email_notification(true)
        .unwrap()
        .unwrap();
    assert_eq!(mail.event_key, key);
    assert_eq!(mail.attempts, 1);
}

fn provider(
    f: &mut TaskPoolFixture,
) -> (
    git_fixture::GitFixture,
    gitlab_source_fixture::GitlabSourceFixture,
    GitNetworkPolicy,
    String,
) {
    use nexora_platform_store::{ProviderSecretContext, SecretGenerator};
    let git = git_fixture::GitFixture::new("workflow-provider");
    let provider = gitlab_source_fixture::GitlabSourceFixture::start(&git, &"a".repeat(40));
    let policy = GitNetworkPolicy {
        gitlab_origin: provider.origin().into(),
        ..Default::default()
    };
    f.work
        .store
        .configure_gitlab(
            &f.work.ctx,
            &GitlabClientRegistration {
                client_id: "fixture-client".into(),
                client_secret: SecretGenerator::token("fixture_client").unwrap(),
                redirect_uri: format!("{}/api/v1/git/gitlab/callback", policy.callback_origin),
                enabled: true,
                expected_revision: 0,
            },
            &policy,
        )
        .unwrap();
    let repository = f
        .work
        .store
        .register_repository(
            &f.work.ctx,
            &f.work.project.id,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "fixture/source".into(),
            },
        )
        .unwrap();
    let connection = SecretGenerator::id("conn").unwrap();
    let host = policy.host().unwrap();
    let tokens=zeroize::Zeroizing::new(serde_json::to_vec(&json!({"access_token":provider.token(),"refresh_token":null,"token_type":"Bearer","scope":"read_api read_repository","expires_in":3600})).unwrap());
    let encrypted = json!(
        f.work
            .store
            .provider_secret_cipher()
            .unwrap()
            .seal(
                &ProviderSecretContext {
                    binding_id: connection.clone(),
                    provider: GitProvider::Gitlab.into(),
                    host: host.clone(),
                    purpose: "gitlab_tokens".into(),
                    revision: 1
                },
                &tokens
            )
            .unwrap()
    );
    let c = &f.work.config;
    let password = zeroize::Zeroizing::new(
        std::fs::read_to_string(c.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&c.host)
        .port(c.postgres_port)
        .user(&c.postgres_user)
        .password(password.trim())
        .dbname(&c.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO {}", f.work.namespace))
        .unwrap();
    sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab',$2,$3,'42','fixture','active',1,$4,now()+interval '1 hour')",&[&connection,&host,&f.work.ctx.actor_id(),&encrypted]).unwrap();
    sql.execute("UPDATE project_repositories SET host=$3,remote_id='101',remote_full_name='fixture/source',connection_id=$1,authorization_status='authorized',revision=2 WHERE id=$2",&[&connection,&repository.id,&host]).unwrap();
    (git, provider, policy, repository.id)
}
#[test]
#[ignore = "requires local PostgreSQL and generated local GitLab fixture; no remote provider"]
fn lost_pr_response_is_reconciled_and_divergent_target_does_not_require_ancestry() {
    let (mut f, task) = setup(
        json!([{"kind":"verify_push"},{"kind":"create_pull_request","source_branch":"develop","target_branch":"stage"},{"kind":"advance_state"}]),
    );
    let (git, provider, policy, repository) = provider(&mut f);
    let mode = json!({"workflow":true,"branches":{"feature/work":"a".repeat(40),"develop":"c".repeat(40),"stage":"b".repeat(40)},"pr_lost_response":true});
    provider.control(mode.clone());
    let input = TaskTransitionRequest {
        expected_revision: task.revision,
        branches: vec![
            TaskTransitionBranch {
                repository_id: repository.clone(),
                branch_name: "feature/work".into(),
                expected_head_sha: "a".repeat(40),
            },
            TaskTransitionBranch {
                repository_id: repository,
                branch_name: "develop".into(),
                expected_head_sha: "c".repeat(40),
            },
        ],
        ..Default::default()
    };
    assert!(
        f.work
            .store
            .transition_pool_task_with_policy(&f.work.ctx, &task.id, "finish", &input, &policy)
            .is_err()
    );
    assert_eq!(
        f.work
            .store
            .pool_task(&f.work.ctx, &task.id)
            .unwrap()
            .revision,
        task.revision
    );
    let runs = f
        .work
        .store
        .task_transition_runs(&f.work.ctx, &task.id)
        .unwrap();
    let run = runs["items"][0]["id"].as_str().unwrap();
    let result = f
        .work
        .store
        .retry_task_transition(&f.work.ctx, &task.id, run, &policy)
        .unwrap();
    assert_eq!(result.revision, task.revision + 1);
    assert_eq!(
        std::fs::read_to_string(git.directory.path().join("provider-control.pr-count")).unwrap(),
        "1"
    );
    let replay = f
        .work
        .store
        .retry_task_transition(&f.work.ctx, &task.id, run, &policy)
        .unwrap();
    assert_eq!(result.revision, replay.revision);
}
#[test]
#[ignore = "requires local PostgreSQL and generated local GitLab fixture; provider facts override declarations"]
fn changed_provider_head_and_missing_base_fail_before_state_or_pr() {
    let (mut f, task) = setup(json!([{"kind":"verify_push"},{"kind":"advance_state"}]));
    let (git, provider, policy, repository) = provider(&mut f);
    provider.control(json!({"workflow":true,"branches":{"feature/work":"d".repeat(40),"develop":"c".repeat(40)}}));
    let input = TaskTransitionRequest {
        expected_revision: task.revision,
        branches: vec![TaskTransitionBranch {
            repository_id: repository,
            branch_name: "feature/work".into(),
            expected_head_sha: "a".repeat(40),
        }],
        ..Default::default()
    };
    assert!(
        f.work
            .store
            .transition_pool_task_with_policy(&f.work.ctx, &task.id, "finish", &input, &policy)
            .is_err()
    );
    let runs = f
        .work
        .store
        .task_transition_runs(&f.work.ctx, &task.id)
        .unwrap();
    assert_eq!(runs["items"][0]["error_code"], "provider_head_changed");
    provider.control(json!({"workflow":true,"branches":{"feature/work":"a".repeat(40)}}));
    let id = runs["items"][0]["id"].as_str().unwrap();
    assert!(
        f.work
            .store
            .retry_task_transition(&f.work.ctx, &task.id, id, &policy)
            .is_err()
    );
    assert_eq!(
        f.work
            .store
            .task_transition_runs(&f.work.ctx, &task.id)
            .unwrap()["items"][0]["error_code"],
        "provider_resource_missing"
    );
    assert_eq!(
        f.work
            .store
            .pool_task(&f.work.ctx, &task.id)
            .unwrap()
            .revision,
        task.revision
    );
    assert!(
        !git.directory
            .path()
            .join("provider-control.pr-count")
            .exists()
    );
}

#[test]
#[ignore = "requires local PostgreSQL; human approval cannot be self-granted or granted through an API key"]
fn self_approval_and_api_key_grants_are_rejected() {
    let (mut f, task) = setup(json!([{"kind":"require_approval"},{"kind":"advance_state"}]));
    let parameters = TaskTransitionRequest {
        expected_revision: task.revision,
        ..Default::default()
    };
    assert!(
        f.work
            .store
            .transition_pool_task(&f.work.ctx, &task.id, "finish", &parameters)
            .is_err()
    );
    let grant = TaskApprovalGrant {
        transition_id: "finish".into(),
        parameters,
        reason: "Self review attempt".into(),
        expires_in_seconds: 600,
    };
    assert!(matches!(
        f.work
            .store
            .grant_pool_task_approval(&f.work.ctx, &task.id, &grant),
        Err(PlatformError::Forbidden)
    ));
    let api = AuthContext::new(
        f.work.ctx.actor_id().into(),
        ActorKind::Human,
        AuthMethod::ApiKey,
        f.work.ctx.credential_id().into(),
        f.work.ctx.grants().to_vec(),
        None,
    );
    assert!(matches!(
        f.work
            .store
            .grant_pool_task_approval(&api, &task.id, &grant),
        Err(PlatformError::Forbidden | PlatformError::Unauthorized)
    ));
    assert_eq!(
        f.work
            .store
            .pool_task(&f.work.ctx, &task.id)
            .unwrap()
            .revision,
        task.revision
    );
}

#[test]
#[ignore = "requires local PostgreSQL; ordered approval must checkpoint earlier actions"]
fn ordered_approval_waits_after_the_preceding_reviewer_notification() {
    let (mut f, task) = setup_governed(json!([
        {"kind":"notify","channel":"in_app","audience":"approvers"},
        {"kind":"require_approval"},
        {"kind":"advance_state"}
    ]));
    let input = TaskTransitionRequest {
        expected_revision: task.revision,
        ..Default::default()
    };
    assert!(
        f.work
            .store
            .transition_pool_task(&f.developer, &task.id, "finish", &input)
            .is_err()
    );
    let runs = f
        .work
        .store
        .task_transition_runs(&f.developer, &task.id)
        .unwrap();
    let run = &runs["items"][0];
    assert_eq!(run["status"], "waiting_approval");
    assert_eq!(run["next_step"], 1);
    assert_eq!(run["outcomes"][0]["kind"], "notify");
    assert_eq!(run["outcomes"][0]["result"]["audience"], "approvers");
    assert!(run["approval_request_id"].is_string());
}

fn setup_governed(steps: Value) -> (TaskPoolFixture, WorkTask) {
    let mut f = TaskPoolFixture::new(false);
    let flow=f.work.store.create_developer_workflow(&f.work.ctx,&serde_json::from_value(json!({"name":"Ordered execution","graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","from":"ready","to":"done","steps":steps}]}})).unwrap()).unwrap();
    let version = f
        .work
        .store
        .publish_developer_workflow(
            &f.work.ctx,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Publish rules".into(),
            },
        )
        .unwrap();
    let assigned = f
        .work
        .store
        .actor_workflow(&f.work.ctx, f.developer.actor_id())
        .unwrap();
    f.work
        .store
        .assign_actor_workflow(
            &f.work.ctx,
            f.developer.actor_id(),
            &DeveloperWorkflowChange {
                expected_revision: assigned.unwrap().revision,
                workflow_version_id: version.id,
                reason: "Use ordered execution".into(),
            },
        )
        .unwrap();
    f.work
        .store
        .create_catalog_task_type(
            &f.work.ctx,
            &CatalogTaskTypeCreate {
                code: "pipeline_work".into(),
                name: "Pipeline work".into(),
                purpose: TaskPurpose::Implementation,
                description: None,
                color: None,
                icon: TaskTypeIcon::default(),
            },
        )
        .unwrap();
    let task = f
        .work
        .store
        .create_pool_task(
            &f.work.ctx,
            &PoolTaskCreate {
                project_id: Some(f.work.project.id.clone()),
                definition: TaskCreate {
                    title: "Ordered governed pipeline".into(),
                    type_code: "pipeline_work".into(),
                    origin: "test".into(),
                    assignee_id: Some(f.developer.actor_id().into()),
                    entry_workflow_id: Some(flow.id.clone()),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let task = f
        .work
        .store
        .take_task(
            &f.developer,
            &task.id,
            &TaskTake {
                expected_revision: task.revision,
                workflow_id: Some(flow.id),
            },
            None,
        )
        .unwrap();
    (f, task)
}

#[test]
#[ignore = "requires local PostgreSQL and generated GitLab server; no live provider"]
fn take_creates_exact_remote_branches_and_reconciles_lost_response_once() {
    let (mut f, template) = setup_governed(json!([{"kind":"advance_state"}]));
    let (git, provider, policy, repository) = provider(&mut f);
    provider.control(json!({"workflow":true,"branches":{"develop":"a".repeat(40)},"branch_lost_response":true,"branch_read_lost_once":true}));
    let owner_assignment = f
        .work
        .store
        .actor_workflow(&f.work.ctx, f.work.ctx.actor_id())
        .unwrap();
    f.work
        .store
        .assign_actor_workflow(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            &DeveloperWorkflowChange {
                expected_revision: owner_assignment.map_or(0, |w| w.revision),
                workflow_version_id: template.workflow_version_id.clone().unwrap(),
                reason: "Start selected repository work".into(),
            },
        )
        .unwrap();
    let task = f
        .work
        .store
        .create_pool_task(
            &f.work.ctx,
            &PoolTaskCreate {
                project_id: Some(f.work.project.id.clone()),
                definition: TaskCreate {
                    title: "Provider-created task branch".into(),
                    type_code: "pipeline_work".into(),
                    origin: "test".into(),
                    assignee_id: Some(f.work.ctx.actor_id().into()),
                    entry_workflow_id: template.workflow_id.clone(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let missing = TaskCodeTake {
        expected_revision: task.revision,
        workflow_id: task.entry_workflow_id.clone(),
        repository_ids: vec![],
    };
    assert!(
        f.work
            .store
            .take_task_with_repositories(&f.work.ctx, &task.id, &missing, None, &policy)
            .is_err()
    );
    assert_eq!(
        f.work
            .store
            .pool_task(&f.work.ctx, &task.id)
            .unwrap()
            .lifecycle,
        TaskLifecycle::Pending
    );
    let input = TaskCodeTake {
        repository_ids: vec![repository],
        ..missing
    };
    let ctx = f.work.ctx.clone();
    let canonical = json!({"task_id":task.id,"take":input});
    let first = f.work.store.request(
        &ctx,
        "task.take",
        "take-provider-repositories",
        &canonical,
        |store| store.take_task_with_repositories(&ctx, &task.id, &input, None, &policy),
    );
    assert!(first.is_err());
    let partial = f.work.store.pool_task(&ctx, &task.id).unwrap();
    let advanced = f.work.store.transition_pool_task(
        &ctx,
        &task.id,
        "finish",
        &TaskTransitionRequest {
            expected_revision: partial.revision,
            ..Default::default()
        },
    );
    assert!(
        matches!(advanced,Err(PlatformError::Conflict(message)) if message.contains("preparation is incomplete"))
    );
    provider.control(json!({"workflow":true,"branches":{"develop":"b".repeat(40)}}));
    let result = f
        .work
        .store
        .request(
            &ctx,
            "task.take",
            "take-provider-repositories",
            &canonical,
            |store| store.take_task_with_repositories(&ctx, &task.id, &input, None, &policy),
        )
        .unwrap();
    assert_eq!(result["lifecycle"], "active");
    assert_eq!(result["repository_branches"][0]["head_sha"], "a".repeat(40));
    assert!(
        result["repository_branches"][0]["branch_name"]
            .as_str()
            .unwrap()
            .starts_with(&format!("feature/{}-", task.id))
    );
    let replay = f
        .work
        .store
        .request(
            &ctx,
            "task.take",
            "take-provider-repositories-new-request",
            &canonical,
            |store| store.take_task_with_repositories(&ctx, &task.id, &input, None, &policy),
        )
        .unwrap();
    assert_eq!(result["start_run_id"], replay["start_run_id"]);
    assert_eq!(
        std::fs::read_to_string(git.directory.path().join("provider-control.branch-count"))
            .unwrap(),
        "1"
    );
    let current = f.work.store.pool_task(&ctx, &task.id).unwrap();
    let correction = |workflow: Option<String>, sha: String| TaskSubtaskCreate {
        expected_revision: current.revision,
        work_kind: TaskSubtaskKind::Corrective,
        tested_repository_id: Some(input.repository_ids[0].clone()),
        tested_commit_sha: Some(sha),
        definition: TaskCreate {
            title: "Corrective proof boundary".into(),
            type_code: "pipeline_work".into(),
            entry_workflow_id: workflow,
            origin: "test".into(),
            ..Default::default()
        },
    };
    let fake = f.work.store.create_task_subtask(
        &ctx,
        &task.id,
        &correction(Some(f.version.workflow_id.clone()), "f".repeat(40)),
    );
    assert!(
        matches!(fake,Err(PlatformError::Conflict(message)) if message.contains("tested repository revision"))
    );
    let same_stage = f.work.store.create_task_subtask(
        &ctx,
        &task.id,
        &correction(current.workflow_id.clone(), "a".repeat(40)),
    );
    assert!(
        matches!(same_stage,Err(PlatformError::Validation(message)) if message.contains("different workflow"))
    );
    let child = f
        .work
        .store
        .create_task_subtask(
            &ctx,
            &task.id,
            &TaskSubtaskCreate {
                expected_revision: current.revision,
                work_kind: TaskSubtaskKind::Decomposition,
                tested_repository_id: None,
                tested_commit_sha: None,
                definition: TaskCreate {
                    title: "Child uses the live parent branch".into(),
                    type_code: "pipeline_work".into(),
                    entry_workflow_id: current.workflow_id.clone(),
                    assignee_id: Some(ctx.actor_id().into()),
                    origin: "test".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let parent_branch = result["repository_branches"][0]["branch_name"]
        .as_str()
        .unwrap();
    provider.control(json!({"workflow":true,"branches":{"develop":"b".repeat(40),(parent_branch):"b".repeat(40)}}));
    let child_take = TaskCodeTake {
        expected_revision: child.revision,
        workflow_id: child.entry_workflow_id.clone(),
        repository_ids: input.repository_ids.clone(),
    };
    let child_result = f
        .work
        .store
        .request(
            &ctx,
            "task.take",
            "child-uses-live-parent-head",
            &json!({"task_id":child.id,"take":child_take}),
            |store| store.take_task_with_repositories(&ctx, &child.id, &child_take, None, &policy),
        )
        .unwrap();
    assert_eq!(
        child_result["repository_branches"][0]["base_branch"],
        parent_branch
    );
    assert_eq!(
        child_result["repository_branches"][0]["base_sha"],
        "b".repeat(40)
    );
    assert_ne!(
        child_result["repository_branches"][0]["branch_name"],
        parent_branch
    );
}

#[test]
#[ignore = "requires local PostgreSQL and generated GitLab; branch write races a reassignment"]
fn take_does_not_register_an_old_branch_after_reassignment_during_provider_write() {
    let (mut f, template) = setup_governed(json!([{"kind":"advance_state"}]));
    let (git, provider, policy, repository) = provider(&mut f);
    provider.control(
        json!({"workflow":true,"branches":{"develop":"a".repeat(40)},"pause_branch_creation":true}),
    );
    let assignment = f
        .work
        .store
        .actor_workflow(&f.work.ctx, f.work.ctx.actor_id())
        .unwrap();
    f.work
        .store
        .assign_actor_workflow(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            &DeveloperWorkflowChange {
                expected_revision: assignment.map_or(0, |w| w.revision),
                workflow_version_id: template.workflow_version_id.clone().unwrap(),
                reason: "Start selected repository work".into(),
            },
        )
        .unwrap();
    let task = f
        .work
        .store
        .create_pool_task(
            &f.work.ctx,
            &PoolTaskCreate {
                project_id: Some(f.work.project.id.clone()),
                definition: TaskCreate {
                    title: "Concurrent assignment boundary".into(),
                    type_code: "pipeline_work".into(),
                    origin: "test".into(),
                    assignee_id: Some(f.work.ctx.actor_id().into()),
                    entry_workflow_id: template.workflow_id.clone(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let input = TaskCodeTake {
        expected_revision: task.revision,
        workflow_id: task.entry_workflow_id.clone(),
        repository_ids: vec![repository],
    };
    let (config, namespace, ctx, id) = (
        f.work.config.clone(),
        f.work.namespace.clone(),
        f.work.ctx.clone(),
        task.id.clone(),
    );
    let worker = std::thread::spawn(move || {
        let mut store = nexora_platform_store::PlatformStore::connect(&config, &namespace).unwrap();
        store.request(
            &ctx,
            "task.take",
            "take-concurrent-reassignment",
            &json!({"task_id":id,"take":input}),
            |store| store.take_task_with_repositories(&ctx, &id, &input, None, &policy),
        )
    });
    let created = git.directory.path().join("provider-control.branch-count");
    for _ in 0..500 {
        if created.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(created.exists(), "fixture did not reach the provider write");
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
    sql.batch_execute(&format!("SET search_path TO {}", f.work.namespace))
        .unwrap();
    sql.execute(
        "UPDATE tasks SET assignee_id=$2,revision=revision+1 WHERE id=$1",
        &[&task.id, &f.other.actor_id()],
    )
    .unwrap();
    std::fs::write(
        git.directory.path().join("provider-control.release-branch"),
        b"",
    )
    .unwrap();
    assert!(worker.join().unwrap().is_err());
    let registered: i64 = sql
        .query_one(
            "SELECT count(*) FROM task_repository_branches WHERE task_id=$1",
            &[&task.id],
        )
        .unwrap()
        .get(0);
    assert_eq!(
        registered, 0,
        "old branch metadata was registered after reassignment"
    );
    assert_eq!(std::fs::read_to_string(created).unwrap(), "1");
}

#[test]
#[ignore = "requires local PostgreSQL and generated GitLab; restores provider authority after AdvanceState crash"]
fn handoff_resume_after_advance_refreshes_provider_evidence_and_commits_once() {
    let (mut f, _) = setup_governed(json!([{"kind":"advance_state"}]));
    let (_git, provider, policy, repository) = provider(&mut f);
    let target=f.work.store.create_developer_workflow(&f.work.ctx,&serde_json::from_value(json!({"name":"QA target","graph":{"states":[{"id":"qa_ready","name":"QA ready","initial":true,"required_base_branch":"qa"},{"id":"qa_done","name":"QA done","terminal":true}],"transitions":[{"id":"qa_finish","from":"qa_ready","to":"qa_done"}]}})).unwrap()).unwrap();
    f.work
        .store
        .publish_developer_workflow(
            &f.work.ctx,
            &target.id,
            &WorkflowPublish {
                expected_revision: target.revision,
                reason: "Publish destination".into(),
            },
        )
        .unwrap();
    let source=f.work.store.create_developer_workflow(&f.work.ctx,&serde_json::from_value(json!({"name":"Crash recovery source","graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","from":"ready","to":"done"},{"id":"deliver","from":"done","to":"done","automatic":true,"steps":[{"kind":"verify_pull_request_merged","target_branch":"qa"},{"kind":"verify_deployment","environment":"qa","target_branch":"qa"},{"kind":"advance_state"},{"kind":"handoff","workflow_id":target.id}]}]}})).unwrap()).unwrap();
    let version = f
        .work
        .store
        .publish_developer_workflow(
            &f.work.ctx,
            &source.id,
            &WorkflowPublish {
                expected_revision: source.revision,
                reason: "Publish final pipeline".into(),
            },
        )
        .unwrap();
    let assignment = f
        .work
        .store
        .actor_workflow(&f.work.ctx, f.work.ctx.actor_id())
        .unwrap();
    f.work
        .store
        .assign_actor_workflow(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            &DeveloperWorkflowChange {
                expected_revision: assignment.map_or(0, |a| a.revision),
                workflow_version_id: version.id.clone(),
                reason: "Execute crash recovery".into(),
            },
        )
        .unwrap();
    let task = f
        .work
        .store
        .create_pool_task(
            &f.work.ctx,
            &PoolTaskCreate {
                project_id: Some(f.work.project.id.clone()),
                definition: TaskCreate {
                    title: "Resume handoff after committed advance".into(),
                    type_code: "pipeline_work".into(),
                    entry_workflow_id: Some(source.id),
                    assignee_id: Some(f.work.ctx.actor_id().into()),
                    origin: "test".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let task = f
        .work
        .store
        .take_task(
            &f.work.ctx,
            &task.id,
            &TaskTake {
                expected_revision: task.revision,
                workflow_id: task.entry_workflow_id.clone(),
            },
            None,
        )
        .unwrap();
    let head = "a".repeat(40);
    let merged = "b".repeat(40);
    let provider_mode = json!({"workflow":true,"branches":{"work/task":head,"qa":merged},"merged_requests":[{"iid":7,"title":"Task merged to QA","source_branch":"work/task","target_branch":"qa","sha":head,"state":"merged","merge_commit_sha":merged}],"deployments":[{"id":9,"sha":merged,"status":"success","environment":{"name":"qa"}}]});
    provider.control(provider_mode.clone());
    let input = TaskTransitionRequest {
        expected_revision: task.revision,
        branches: vec![TaskTransitionBranch {
            repository_id: repository.clone(),
            branch_name: "work/task".into(),
            expected_head_sha: head.clone(),
        }],
        ..Default::default()
    };
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
    sql.batch_execute(&format!("SET search_path TO {}", f.work.namespace))
        .unwrap();
    // Reproduce the durable boundary left by AdvanceState: state/result/revision and next_step committed,
    // while a new process has no in-memory provider facts.
    let advanced:Value=sql.query_one("UPDATE tasks t SET state_id='done',revision=revision+1 WHERE id=$1 RETURNING to_jsonb(t)",&[&task.id]).unwrap().get(0);
    sql.execute("INSERT INTO task_repository_branches(task_id,repository_id,branch_name,base_branch,base_sha,head_sha,evidence) VALUES($1,$2,'work/task','develop',$3,$3,'{\"provenance\":\"provider_verified\"}')",&[&task.id,&repository,&head]).unwrap();
    let merge_receipt = json!({"repository_id":repository,"source_branch":"work/task","head_sha":head,"merge_sha":merged,"target_branch":"qa","status":"merged","provenance":"provider_verified"});
    let deploy_receipt = json!({"repository_id":repository,"source_branch":"work/task","head_sha":merged,"source_head_sha":head,"environment":"qa","status":"success","provenance":"provider_verified","merge":merge_receipt});
    let id = nexora_platform_store::SecretGenerator::id("trn").unwrap();
    let run = json!({"id":id,"task_id":task.id,"actor_id":f.work.ctx.actor_id(),"request_id":"resume-handoff-after-advance","request_operation":"task.transition","transition_id":"deliver","task_revision":task.revision,"workflow_version_id":version.id,"status":"failed","next_step":3,"advanced":true,"provider_intents":{},"result":advanced,"outcomes":[{"step_index":0,"kind":"verify_pull_request_merged","result":{"pull_requests":[merge_receipt]}},{"step_index":1,"kind":"verify_deployment","result":{"deployments":[deploy_receipt]}},{"step_index":2,"kind":"advance_state","result":{"task_revision":task.revision+1}}]});
    let fingerprint = {
        use sha2::Digest;
        format!(
            "{:x}",
            sha2::Sha256::digest(serde_json::to_vec(&json!(input)).unwrap())
        )
    };
    sql.execute("INSERT INTO task_transition_runs(id,task_id,project_id,actor_id,credential_id,auth_method,task_revision,transition_id,fingerprint,parameters,record) VALUES($1,$2,$3,$4,$5,'session',$6,'deliver',$7,$8,$9)",&[&id,&task.id,&task.project_id,&f.work.ctx.actor_id(),&f.work.ctx.credential_id(),&task.revision,&fingerprint,&json!(input),&run]).unwrap();
    let ctx = f.work.ctx.clone();
    let canonical = json!({"task_id":task.id,"transition_id":"deliver","request":input});
    let mut failed_deployment = provider_mode.clone();
    failed_deployment["deployments"][0]["status"] = json!("failed");
    provider.control(failed_deployment);
    let blocked = f.work.store.request(
        &ctx,
        "task.transition",
        "resume-handoff-after-advance",
        &canonical,
        |store| store.transition_pool_task_with_policy(&ctx, &task.id, "deliver", &input, &policy),
    );
    assert!(blocked.is_err());
    assert_eq!(
        f.work.store.pool_task(&ctx, &task.id).unwrap().workflow_id,
        task.workflow_id
    );
    provider.control(provider_mode);
    let result = f
        .work
        .store
        .request(
            &ctx,
            "task.transition",
            "resume-handoff-after-advance",
            &canonical,
            |store| {
                store.transition_pool_task_with_policy(&ctx, &task.id, "deliver", &input, &policy)
            },
        )
        .unwrap();
    assert_eq!(result.workflow_id.as_deref(), Some(target.id.as_str()));
    assert_eq!(result.state_id.as_deref(), Some("qa_ready"));
    assert!(result.assignee_id.is_none());
    let replay = f
        .work
        .store
        .request(
            &ctx,
            "task.transition",
            "resume-handoff-after-advance",
            &canonical,
            |store| {
                store.transition_pool_task_with_policy(&ctx, &task.id, "deliver", &input, &policy)
            },
        )
        .unwrap();
    assert_eq!(replay.revision, result.revision);
    let deliveries:i64=sql.query_one("SELECT count(*) FROM task_history WHERE task_id=$1 AND operation='task.workflow_delivered'",&[&task.id]).unwrap().get(0);
    assert_eq!(deliveries, 1);
}
