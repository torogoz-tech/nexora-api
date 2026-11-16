#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{body::Body, http::Request};
use base64::{Engine, engine::general_purpose::STANDARD};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::{Value, json};
use tower::ServiceExt;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; MCP uploads, project scope, idempotency and acceptance criteria"]
fn task_upload_tools_are_durable_scoped_and_idempotent() {
    let mut f = WorkItemFixture::new();
    let task = f
        .store
        .create_pool_task(
            &f.ctx,
            &PoolTaskCreate {
                project_id: Some(f.project.id.clone()),
                definition: TaskCreate {
                    title: "MCP attachments".into(),
                    type_code: "task".into(),
                    origin: "test".into(),
                    ..Default::default()
                },
            },
        )
        .unwrap();
    let scope = |write| KeyScope {
        capabilities: if write {
            vec![Capability::TasksRead, Capability::TasksWrite]
        } else {
            vec![Capability::TasksRead]
        },
        project_ids: vec![f.project.id.clone()],
        all_projects: false,
        installation: false,
    };
    let key = f
        .store
        .issue_key(&f.ctx, f.ctx.actor_id(), "Upload", scope(true), 3600)
        .unwrap();
    let reader = f
        .store
        .issue_key(&f.ctx, f.ctx.actor_id(), "Read", scope(false), 3600)
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
        runtime.block_on(async{
  let request=Request::builder().method("POST").uri("/mcp").header("authorization",format!("Bearer {token}")).header("content-type","application/json").header("accept","application/json, text/event-stream").header("mcp-protocol-version","2025-11-25").body(Body::from(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}).to_string())).unwrap();
  let response=app.clone().oneshot(request).await.unwrap();
  let bytes=response.into_body().collect().await.unwrap().to_bytes();serde_json::from_slice::<Value>(&bytes).unwrap()
 })
    };
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(20, 20)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let image_args = json!({"task_id":task.id,"name":"screen.png","data_base64":STANDARD.encode(png.into_inner()),"request_id":"image-upload"});
    let uploaded = call(&key.key, "nexora_task_image_upload", image_args.clone());
    assert_eq!(uploaded["result"]["isError"], false, "{uploaded}");
    assert_eq!(
        uploaded,
        call(&key.key, "nexora_task_image_upload", image_args.clone())
    );
    assert_eq!(
        call(&reader.key, "nexora_task_image_upload", image_args)["result"]["isError"],
        true
    );
    let image = &uploaded["result"]["structuredContent"];
    assert!(image["markdown"].as_str().unwrap().starts_with("!["));
    let changed = call(
        &key.key,
        "nexora_task_change",
        json!({"project_id":f.project.id,"task_id":task.id,"request_id":"criteria","change":{"expected_revision":task.revision,"acceptance_criteria":["**First**","Second"],"context":image["markdown"],"reason":"Document acceptance"}}),
    );
    assert_eq!(changed["result"]["isError"], false, "{changed}");
    assert_eq!(
        changed["result"]["structuredContent"]["acceptance_criteria"],
        json!(["**First**", "Second"])
    );
    let file_args = json!({"task_id":task.id,"name":"large.pdf","data_base64":STANDARD.encode(vec![b'x';6*1024*1024]),"request_id":"file-upload"});
    let file = call(&key.key, "nexora_task_attachment_upload", file_args.clone());
    assert_eq!(file["result"]["isError"], false, "{file}");
    assert_eq!(
        file,
        call(&key.key, "nexora_task_attachment_upload", file_args)
    );
    let page = call(
        &key.key,
        "nexora_task_attachments_list",
        json!({"task_id":task.id}),
    );
    assert_eq!(
        page["result"]["structuredContent"]["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call(
            &key.key,
            "nexora_task_image_upload",
            json!({"task_id":task.id,"name":"bad.png","data_base64":"%%%","request_id":"bad"})
        )["result"]["isError"],
        true
    );
}
