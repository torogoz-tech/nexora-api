#[path = "../../nexora-platform-store/tests/support/task_pool_fixture.rs"]
mod task_pool_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::{Value, json};
use task_pool_fixture::TaskPoolFixture;
use tower::ServiceExt;
#[test]
#[ignore = "requires PostgreSQL"]
fn rest_and_mcp_list_all_member_workflows() {
    let mut f = TaskPoolFixture::new(false);
    let actor = f.work.ctx.actor_id().to_owned();
    let flow=f.work.store.create_developer_workflow(&f.work.ctx,&serde_json::from_value(json!({"name":"QA","graph":{"states":[{"id":"ready","name":"Ready","initial":true},{"id":"done","name":"Done","terminal":true}],"transitions":[{"id":"finish","name":"Finish","from":"ready","to":"done"}]}})).unwrap()).unwrap();
    let version = f
        .work
        .store
        .publish_developer_workflow(
            &f.work.ctx,
            &flow.id,
            &WorkflowPublish {
                expected_revision: flow.revision,
                reason: "QA".into(),
            },
        )
        .unwrap();
    for version in [&f.version.id, &version.id] {
        f.work
            .store
            .assign_actor_workflow(
                &f.work.ctx,
                &actor,
                &DeveloperWorkflowChange {
                    expected_revision: 0,
                    workflow_version_id: version.clone(),
                    reason: String::new(),
                },
            )
            .unwrap();
    }
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            &actor,
            "Workflow read",
            KeyScope {
                capabilities: vec![Capability::WorkflowsRead],
                project_ids: vec![],
                all_projects: false,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async{
  let app=ApiServer::router(ApiState::new(f.work.config.clone(),f.work.namespace.clone(),2).await.unwrap());
  for (path,body) in [(format!("/api/v1/actors/{actor}/workflows"),None),("/mcp".into(),Some(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nexora_developer_workflows_list","arguments":{}}})))]{
   let request=Request::builder().method(if body.is_some(){"POST"}else{"GET"}).uri(path).header("authorization",format!("Bearer {}",key.key)).header("content-type","application/json").header("accept","application/json, text/event-stream").header("mcp-protocol-version","2025-11-25").body(Body::from(body.as_ref().map(|v|v.to_string()).unwrap_or_default())).unwrap();
   let response=app.clone().oneshot(request).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
   let bytes=response.into_body().collect().await.unwrap().to_bytes();let value:Value=serde_json::from_slice(&bytes).unwrap();
   let rows=if body.is_some(){&value["result"]["structuredContent"]}else{&value};assert_eq!(rows.as_array().unwrap().len(),2);
  }
 });
}
