#[path = "../../nexora-platform-store/tests/support/local_snapshot_fixture.rs"]
mod local_snapshot_fixture;
#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use local_snapshot_fixture::LocalSnapshotFixture;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::{Value, json};
use tower::ServiceExt;
#[test]
#[ignore = "requires local databases; reported Git metadata, no local Git executor"]
fn agent_report_is_scoped_immutable_idempotent_and_separate_from_task_transition() {
    let mut f = LocalSnapshotFixture::new();
    let adopted = f.adopt();
    assert!(
        f.runtime
            .query_header(
                &f.header.tenant_id,
                &f.header.repository_id,
                Some(&f.header.id)
            )
            .is_ok()
    );
    let repo = adopted.repository_id;
    let project = f.work.project.id.clone();
    let ctx = f.work.ctx.clone();
    let flow=f.work.store.create_workflow(&ctx,&project,&serde_json::from_value(json!({"name":"Implementation","purpose":"Report branch before editing","resource_kinds":["task"],"graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"doing","name":"Doing","terminal":true}],"transitions":[{"id":"start","name":"Start","from":"ready","to":"doing","guards":[{"kind":"working_branch_reported"}]}]}})).unwrap()).unwrap();
    let version = f
        .work
        .store
        .publish_workflow(
            &ctx,
            &project,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "Test policy".into(),
            },
        )
        .unwrap();
    let task = f
        .work
        .store
        .create_task(
            &ctx,
            &project,
            &TaskCreate {
                title: "Implement fixture".into(),
                type_code: "task".into(),
                workflow_version_id: Some(version.id.clone()),
                origin: "Fixture".into(),
                fields: serde_json::from_value(
                    json!({"working_branch_reported":true,"working_branch_verified":true}),
                )
                .unwrap(),
                ..Default::default()
            },
        )
        .unwrap();
    let params = TaskTransitionRequest {
        expected_revision: task.revision,
        ..Default::default()
    };
    assert!(
        f.work
            .store
            .transition_task(&ctx, &project, &task.id, "start", &params)
            .is_err(),
        "client fields cannot forge persisted report"
    );
    let key = f
        .work
        .store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Branch fixture",
            KeyScope {
                capabilities: Capability::all(),
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let restricted = f
        .work
        .store
        .issue_key(
            &ctx,
            ctx.actor_id(),
            "Read only",
            KeyScope {
                capabilities: vec![
                    Capability::ProjectsRead,
                    Capability::TasksRead,
                    Capability::RepositoriesRead,
                    Capability::KnowledgeRead,
                ],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let app = rt.block_on(async {
        ApiServer::router(
            ApiState::new(f.work.config.clone(), f.work.namespace.clone(), 2)
                .await
                .unwrap(),
        )
    });
    let call = |token: &str, name: &str, args: Value| -> Value {
        rt.block_on(async{let req=Request::builder().method("POST").uri("/mcp").header("authorization",format!("Bearer {token}")).header("content-type","application/json").header("accept","application/json, text/event-stream").header("mcp-protocol-version","2025-11-25").body(Body::from(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}).to_string())).unwrap();let res=app.clone().oneshot(req).await.unwrap();let bytes=res.into_body().collect().await.unwrap().to_bytes();serde_json::from_slice(&bytes).unwrap()})
    };
    let args = json!({"project_id":project,"task_id":task.id,"request_id":"prepare-1","preparation":{"transition_id":"start","parameters":params,"repository_ids":[repo]}});
    assert_eq!(
        call(&restricted.key, "nexora_task_work_prepare", args.clone())["result"]["isError"],
        true
    );
    let result = call(&key.key, "nexora_task_work_prepare", args.clone());
    assert_eq!(result["result"]["isError"], false, "{result}");
    let session = result["result"]["structuredContent"].clone();
    assert!(session["report"].is_null());
    assert_eq!(session["branches"][0]["base_branch"], "develop");
    assert_eq!(
        call(&key.key, "nexora_task_work_prepare", args)["result"]["structuredContent"],
        session
    );
    let query = json!({"project_id":project,"task_id":task.id,"session_id":session["id"]});
    assert_eq!(
        call(&key.key, "nexora_task_work_start", query.clone())["error"]["code"],
        -32602,
        "no Git execution tool"
    );
    let report = json!({"project_id":project,"task_id":task.id,"session_id":session["id"],"request_id":"report-1","report":{"branches":[{"repository_id":repo,"branch_name":session["branches"][0]["branch_name"],"base_sha":"a".repeat(40),"head_sha":"a".repeat(40)}]}});
    let other = f
        .work
        .store
        .create_actor(&ctx, ActorKind::Agent, "Other actor", "Test isolation")
        .unwrap();
    let caps = vec![
        Capability::ProjectsRead,
        Capability::TasksRead,
        Capability::TasksWrite,
        Capability::TasksTransition,
        Capability::RepositoriesRead,
        Capability::KnowledgeRead,
    ];
    for cap in &caps {
        f.work
            .store
            .grant(&ctx, &other.id, *cap, Some(&project))
            .unwrap();
    }
    let other_key = f
        .work
        .store
        .issue_key(
            &ctx,
            &other.id,
            "Other actor key",
            KeyScope {
                capabilities: caps,
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    assert_eq!(
        call(&other_key.key, "nexora_task_work_get", query.clone())["result"]["isError"],
        true
    );
    assert_eq!(
        call(&other_key.key, "nexora_task_work_report", report.clone())["result"]["isError"],
        true
    );
    assert_eq!(
        call(&restricted.key, "nexora_task_work_report", report.clone())["result"]["isError"],
        true
    );
    let mut wrong_name = report.clone();
    wrong_name["report"]["branches"][0]["branch_name"] = json!("feature/another-session");
    assert_eq!(
        call(&key.key, "nexora_task_work_report", wrong_name)["result"]["isError"],
        true
    );
    let mut duplicate = report.clone();
    let second = duplicate["report"]["branches"][0].clone();
    duplicate["report"]["branches"]
        .as_array_mut()
        .unwrap()
        .push(second);
    assert_eq!(
        call(&key.key, "nexora_task_work_report", duplicate)["result"]["isError"],
        true
    );
    let mut forged = report.clone();
    forged["report"]["verified"] = json!(true);
    assert_eq!(
        call(&key.key, "nexora_task_work_report", forged)["error"]["code"],
        -32602
    );
    let mut local_path = report.clone();
    local_path["report"]["branches"][0]["workspace_path"] = json!("/private/work");
    assert_eq!(
        call(&key.key, "nexora_task_work_report", local_path)["error"]["code"],
        -32602
    );
    let mut invalid = report.clone();
    invalid["report"]["branches"][0]["base_sha"] = json!("not-a-commit");
    assert_eq!(
        call(&key.key, "nexora_task_work_report", invalid)["result"]["isError"],
        true
    );
    let recorded = call(&key.key, "nexora_task_work_report", report.clone());
    assert_eq!(recorded["result"]["isError"], false, "{recorded}");
    let recorded = recorded["result"]["structuredContent"].clone();
    assert_eq!(recorded["report"]["provenance"], "agent_reported");
    assert_eq!(recorded["actor_id"], ctx.actor_id());
    assert_eq!(recorded["report"]["credential_id"], key.metadata.id);
    assert!(recorded.get("verified").is_none());
    assert_eq!(
        f.work
            .store
            .get_task(&ctx, &project, &task.id)
            .unwrap()
            .state_id,
        Some("ready".into()),
        "reporting is not a state transition"
    );
    assert_eq!(
        call(&key.key, "nexora_task_work_report", report.clone())["result"]["structuredContent"],
        recorded
    );
    let mut changed = report.clone();
    changed["request_id"] = json!("alter-report");
    changed["report"]["branches"][0]["head_sha"] = json!("b".repeat(40));
    assert_eq!(
        call(&key.key, "nexora_task_work_report", changed)["result"]["isError"],
        true,
        "report is immutable"
    );
    let transition = json!({"project_id":project,"task_id":task.id,"transition_id":"start","request_id":"start-1","parameters":params});
    let advanced = call(&key.key, "nexora_task_transition", transition.clone());
    assert_eq!(advanced["result"]["isError"], false, "{advanced}");
    assert_eq!(advanced["result"]["structuredContent"]["state_id"], "doing");
    assert_eq!(
        call(&key.key, "nexora_task_transition", transition)["result"]["structuredContent"],
        advanced["result"]["structuredContent"]
    );
    assert_eq!(
        call(&key.key, "nexora_task_work_report", report)["result"]["structuredContent"],
        recorded,
        "receipt replays after transition"
    );
    assert_eq!(f.sql.query_one("SELECT count(*) FROM task_history WHERE task_id=$1 AND operation='task.transitioned'",&[&task.id]).unwrap().get::<_,i64>(0),1);
    let failed_task = f
        .work
        .store
        .create_task(
            &ctx,
            &project,
            &TaskCreate {
                title: "Policy changed".into(),
                type_code: "task".into(),
                workflow_version_id: Some(version.id),
                origin: "Fixture".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let preparation = TaskWorkPrepare {
        transition_id: "start".into(),
        parameters: TaskTransitionRequest {
            expected_revision: 1,
            ..Default::default()
        },
        repository_ids: vec![repo.clone()],
    };
    let stale = f
        .work
        .store
        .prepare_task_work(&ctx, &project, &failed_task.id, &preparation)
        .unwrap();
    f.work
        .store
        .change_development_policy(
            &ctx,
            &project,
            &repo,
            &DevelopmentPolicyChange {
                expected_revision: 0,
                base_branch: "main".into(),
                branch_prefix: "feature".into(),
            },
        )
        .unwrap();
    let rejected = call(
        &key.key,
        "nexora_task_work_report",
        json!({"project_id":project,"task_id":failed_task.id,"session_id":stale.id,"request_id":"stale-report","report":{"branches":[{"repository_id":repo,"branch_name":stale.branches[0].branch_name,"base_sha":"a".repeat(40),"head_sha":"a".repeat(40)}]}}),
    );
    assert_eq!(rejected["result"]["isError"], true);
    assert_eq!(
        f.work
            .store
            .get_task(&ctx, &project, &failed_task.id)
            .unwrap()
            .state_id,
        Some("ready".into())
    );
    // Newest valid report supersedes stale reports, independently of random session-ID order.
    let replacement = f
        .work
        .store
        .prepare_task_work(&ctx, &project, &failed_task.id, &preparation)
        .unwrap();
    let declaration = TaskWorkReport {
        branches: vec![TaskWorkBranchReport {
            repository_id: repo.clone(),
            branch_name: replacement.branches[0].branch_name.clone(),
            base_sha: "a".repeat(40),
            head_sha: "a".repeat(40),
        }],
    };
    f.work
        .store
        .report_task_work(
            &ctx,
            &project,
            &failed_task.id,
            &replacement.id,
            &declaration,
        )
        .unwrap();
    f.sql.execute("UPDATE task_work_sessions SET id='wrk_zzzz_stale',record=jsonb_set(record,'{id}','\"wrk_zzzz_stale\"'::jsonb),created_at=clock_timestamp()-interval '1 day' WHERE id=$1",&[&replacement.id]).unwrap();
    f.work
        .store
        .change_development_policy(
            &ctx,
            &project,
            &repo,
            &DevelopmentPolicyChange {
                expected_revision: 1,
                base_branch: "develop".into(),
                branch_prefix: "feature".into(),
            },
        )
        .unwrap();
    let fresh = f
        .work
        .store
        .prepare_task_work(&ctx, &project, &failed_task.id, &preparation)
        .unwrap();
    let fresh_report = TaskWorkReport {
        branches: vec![TaskWorkBranchReport {
            repository_id: repo.clone(),
            branch_name: fresh.branches[0].branch_name.clone(),
            base_sha: "b".repeat(40),
            head_sha: "b".repeat(40),
        }],
    };
    f.work
        .store
        .report_task_work(&ctx, &project, &failed_task.id, &fresh.id, &fresh_report)
        .unwrap();
    assert_eq!(
        f.work
            .store
            .transition_task(
                &ctx,
                &project,
                &failed_task.id,
                "start",
                &preparation.parameters
            )
            .unwrap()
            .state_id,
        Some("doing".into())
    );
    // A task edit invalidates an unreported preparation.
    let revised = f
        .work
        .store
        .create_task(
            &ctx,
            &project,
            &TaskCreate {
                title: "Revision changed".into(),
                type_code: "task".into(),
                workflow_version_id: Some(fresh.workflow_version_id.clone()),
                origin: "Fixture".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let pending = f
        .work
        .store
        .prepare_task_work(&ctx, &project, &revised.id, &preparation)
        .unwrap();
    f.work
        .store
        .change_task(
            &ctx,
            &project,
            &revised.id,
            &TaskChange {
                expected_revision: 1,
                title: Some("Revised".into()),
                reason: "Test conflict".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        f.work
            .store
            .report_task_work(
                &ctx,
                &project,
                &revised.id,
                &pending.id,
                &TaskWorkReport {
                    branches: vec![TaskWorkBranchReport {
                        repository_id: repo.clone(),
                        branch_name: pending.branches[0].branch_name.clone(),
                        base_sha: "a".repeat(40),
                        head_sha: "a".repeat(40)
                    }]
                }
            )
            .is_err()
    );
    assert!(
        !f.root.join(".git").exists(),
        "the API never initializes or operates a developer Git checkout"
    );
    f.work.store.revoke_key(&ctx, &key.metadata.id).unwrap();
    assert_eq!(
        call(&key.key, "nexora_task_work_get", query)["code"],
        "UNAUTHORIZED"
    );
}
