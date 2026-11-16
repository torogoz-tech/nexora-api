#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::{Value, json};
use tower::ServiceExt;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; governed MCP subtask/dependency mutations and attenuation"]
fn mcp_task_relations_mutations_are_idempotent_and_scoped() {
    let mut f = WorkItemFixture::new();
    let create = |title: &str| PoolTaskCreate {
        project_id: None,
        definition: TaskCreate {
            title: title.into(),
            type_code: "task".into(),
            origin: "test".into(),
            ..Default::default()
        },
    };
    let parent = f.store.create_pool_task(&f.ctx, &create("Parent")).unwrap();
    let blocker = f
        .store
        .create_pool_task(&f.ctx, &create("Blocker"))
        .unwrap();
    let existing = f
        .store
        .create_pool_task(&f.ctx, &create("Existing child"))
        .unwrap();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Relations manager",
            KeyScope {
                capabilities: vec![Capability::TasksRead, Capability::TasksWrite],
                project_ids: vec![],
                all_projects: false,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let narrow = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Project only",
            KeyScope {
                capabilities: vec![Capability::TasksRead, Capability::TasksWrite],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let app = runtime.block_on(async {
        ApiServer::router(
            ApiState::new(f.config.clone(), f.namespace.clone(), 4)
                .await
                .unwrap(),
        )
    });
    let call = |token: &str, name: &str, args: Value| {
        runtime.block_on(async{let req=Request::builder().method("POST").uri("/mcp").header("authorization",format!("Bearer {token}")).header("content-type","application/json").header("accept","application/json, text/event-stream").header("mcp-protocol-version","2025-11-25").body(Body::from(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}).to_string())).unwrap();let response=app.clone().oneshot(req).await.unwrap();let bytes=response.into_body().collect().await.unwrap().to_bytes();serde_json::from_slice::<Value>(&bytes).unwrap()})
    };
    let args = json!({"task_id":parent.id,"request_id":"mcp-subtask-create","subtask":{"expected_revision":parent.revision,"definition":create("MCP child").definition}});
    let child = call(&key.key, "nexora_task_subtask_create", args.clone());
    assert_eq!(child["result"]["isError"], false, "{child}");
    assert_eq!(child, call(&key.key, "nexora_task_subtask_create", args));
    let read = call(
        &key.key,
        "nexora_task_relations_get",
        json!({"task_id":parent.id}),
    );
    let relations = &read["result"]["structuredContent"];
    assert_eq!(relations["subtasks_total"], 1);
    let link_args = json!({"task_id":parent.id,"child_task_id":existing.id,"request_id":"mcp-link","change":{"expected_revision":relations["task_revision"],"child_revision":existing.revision,"reason":"Break down existing work"}});
    let linked = call(&key.key, "nexora_task_subtask_link", link_args.clone());
    assert_eq!(linked["result"]["isError"], false, "{linked}");
    assert_eq!(
        linked,
        call(&key.key, "nexora_task_subtask_link", link_args)
    );
    assert!(
        linked["result"]["structuredContent"]
            .get("subtasks")
            .is_none(),
        "receipts must not cache unrelated task projections"
    );
    let revision = linked["result"]["structuredContent"]["parent_revision"].clone();
    let dep_args = json!({"request_id":"mcp-prerequisite","dependency":{"source_task_id":parent.id,"target_task_id":blocker.id,"source_revision":revision,"target_revision":blocker.revision,"kind":"depends_on","reason":"Finish prerequisite first"}});
    let dependency = call(&key.key, "nexora_task_dependency_create", dep_args.clone());
    assert_eq!(dependency["result"]["isError"], false, "{dependency}");
    assert_eq!(
        dependency,
        call(&key.key, "nexora_task_dependency_create", dep_args)
    );
    let read = call(
        &key.key,
        "nexora_task_relations_get",
        json!({"task_id":parent.id}),
    );
    assert_eq!(read["result"]["structuredContent"]["blocking_count"], 1);
    assert_eq!(
        call(
            &narrow.key,
            "nexora_task_relations_get",
            json!({"task_id":parent.id})
        )["result"]["isError"],
        true
    );
    let dep = &dependency["result"]["structuredContent"];
    let removed = call(
        &key.key,
        "nexora_task_dependency_remove",
        json!({"task_id":parent.id,"dependency_id":dep["id"],"request_id":"mcp-remove-prerequisite","change":{"expected_revision":dep["revision"],"reason":"Explicitly release requirement"}}),
    );
    assert_eq!(removed["result"]["isError"], false, "{removed}");
    let read = call(
        &key.key,
        "nexora_task_relations_get",
        json!({"task_id":parent.id}),
    );
    assert_eq!(read["result"]["structuredContent"]["blocking_count"], 0);
}
