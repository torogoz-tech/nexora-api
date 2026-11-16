#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::{Value, json};
use tower::ServiceExt;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; pasted raster evidence, draft isolation, task ownership and replay"]
fn pasted_images_are_durable_private_task_evidence() {
    let mut f = WorkItemFixture::new();
    let issue = |store: &mut nexora_platform_store::PlatformStore| {
        store
            .issue_key(
                &f.ctx,
                f.ctx.actor_id(),
                "Evidence fixture",
                KeyScope {
                    capabilities: vec![Capability::TasksRead, Capability::TasksWrite],
                    project_ids: vec![],
                    all_projects: false,
                    installation: true,
                },
                3600,
            )
            .unwrap()
    };
    let key = issue(&mut f.store);
    let password = zeroize::Zeroizing::new(
        nexora_platform_store::SecretGenerator::token("evidence_password").unwrap(),
    );
    let actor = f
        .store
        .create_user(&f.ctx, "evidence_reader", &password, "Evidence reader")
        .unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &actor.id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: true,
                project_ids: vec![],
                expected_revision: 0,
            },
        )
        .unwrap();
    let reader = f.store.login("evidence_reader", &password).unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let app = runtime.block_on(async {
        ApiServer::router(
            ApiState::new(f.config.clone(), f.namespace.clone(), 4)
                .await
                .unwrap(),
        )
    });
    let call = |method: &str, path: &str, token: &str, value: Option<Value>, receipt: &str| {
        runtime.block_on(async {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header("authorization", format!("Bearer {token}"))
                        .header("content-type", "application/json")
                        .header("idempotency-key", receipt)
                        .body(value.map_or_else(Body::empty, |v| Body::from(v.to_string())))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let headers = response.headers().clone();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (status, headers, bytes)
        })
    };
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(48, 20)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let upload = json!({"name":"captura.png","image_base64":STANDARD.encode(png.into_inner())});
    let (status, _, body) = call(
        "POST",
        "/api/v1/task-evidence",
        &key.key,
        Some(upload.clone()),
        "paste-image",
    );
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let evidence: Value = serde_json::from_slice(&body).unwrap();
    let url = evidence["url"].as_str().unwrap();
    let replay = call(
        "POST",
        "/api/v1/task-evidence",
        &key.key,
        Some(upload),
        "paste-image",
    );
    assert_eq!(replay.0, StatusCode::OK);
    assert_eq!(replay.2, body);
    assert_eq!(
        call("GET", url, &reader.access_token, None, "read-draft").0,
        StatusCode::NOT_FOUND
    );
    let (status, headers, bytes) = call("GET", url, &key.key, None, "own-draft");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["cache-control"], "private, no-store");
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (48, 20));
    let context = format!("## Evidence\n![Capture](<{url}>)");
    let create = json!({"project_id":null,"title":"Pasted evidence","type_code":"task","origin":"web","context":context,"fields":{}});
    let created = call(
        "POST",
        "/api/v1/tasks",
        &key.key,
        Some(create.clone()),
        "create-evidence-task",
    );
    assert_eq!(
        created.0,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&created.2)
    );
    assert_eq!(
        call("GET", url, &reader.access_token, None, "read-bound").0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            "POST",
            "/api/v1/tasks",
            &key.key,
            Some(create),
            "steal-evidence-other-task"
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let bad =
        json!({"name":"attack.svg","image_base64":STANDARD.encode(b"<svg onload='alert(1)'/>")});
    assert_eq!(
        call(
            "POST",
            "/api/v1/task-evidence",
            &key.key,
            Some(bad),
            "bad-raster"
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let id: Value = serde_json::from_slice(&created.2).unwrap();
    let second = call(
        "POST",
        "/api/v1/task-evidence",
        &key.key,
        Some(json!({"name":"second.png","image_base64":STANDARD.encode(&bytes)})),
        "second-paste",
    );
    assert_eq!(second.0, StatusCode::OK);
    let second: Value = serde_json::from_slice(&second.2).unwrap();
    let second_url = second["url"].as_str().unwrap();
    let edited_context = format!("{context}\n![Second](<{second_url}>)");
    let patched = call(
        "PATCH",
        &format!("/api/v1/tasks/{}", id["id"].as_str().unwrap()),
        &key.key,
        Some(
            json!({"expected_revision":id["revision"],"context":edited_context,"reason":"Paste evidence into an existing task"}),
        ),
        "save-existing-context",
    );
    assert_eq!(
        patched.0,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&patched.2)
    );
    assert_eq!(
        call("GET", second_url, &reader.access_token, None, "read-second").0,
        StatusCode::OK
    );
    let private = call(
        "POST",
        "/api/v1/task-evidence",
        &key.key,
        Some(json!({"name":"private.png","image_base64":STANDARD.encode(&bytes)})),
        "private-paste",
    );
    assert_eq!(private.0, StatusCode::OK);
    let private: Value = serde_json::from_slice(&private.2).unwrap();
    let private_url = private["url"].as_str().unwrap();
    let private_context = format!("![Private](<{private_url}>)");
    let hidden=f.store.create_pool_task(&f.ctx,&serde_json::from_value(json!({"project_id":f.project.id,"title":"Project-private evidence","type_code":"task","origin":"test","context":private_context,"fields":{}})).unwrap()).unwrap();
    assert_eq!(hidden.context, private_context);
    assert_eq!(
        call(
            "GET",
            private_url,
            &reader.access_token,
            None,
            "hidden-evidence"
        )
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call("GET", private_url, &key.key, None, "attenuated-evidence").0,
        StatusCode::NOT_FOUND
    );
    let task = f
        .store
        .pool_task(&f.ctx, id["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(task.context, edited_context);
}
