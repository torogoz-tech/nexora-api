#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use base64::Engine;
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use nexora_platform_store::SecretGenerator;
use serde_json::{Value, json};
use tower::ServiceExt;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; settings, write-only encrypted secrets, branding and SMTP test outbox; no external calls"]
fn settings_preserve_authority_revisions_and_write_only_credentials() {
    let mut f = WorkItemFixture::new();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Settings fixture",
            KeyScope {
                capabilities: vec![Capability::InstallationManage],
                project_ids: vec![],
                all_projects: false,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let limited = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Read fixture",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let app = rt.block_on(async {
        ApiServer::router(
            ApiState::new(f.config.clone(), f.namespace.clone(), 2)
                .await
                .unwrap(),
        )
    });
    let call = |method: &str, path: &str, token: Option<&str>, body: Value, id: Option<&str>| {
        rt.block_on(async {
            let mut request = Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json");
            if let Some(token) = token {
                request = request.header("authorization", format!("Bearer {token}"));
            }
            if let Some(id) = id {
                request = request.header("idempotency-key", id);
            }
            let response = app
                .clone()
                .oneshot(
                    request
                        .body(if method == "GET" {
                            Body::empty()
                        } else {
                            Body::from(serde_json::to_vec(&body).unwrap())
                        })
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (status, serde_json::from_slice::<Value>(&bytes).unwrap())
        })
    };
    let org = "/api/v1/settings/organization";
    assert_eq!(
        call("GET", org, None, Value::Null, None).0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call("GET", org, Some(&limited.key), Value::Null, None).0,
        StatusCode::FORBIDDEN
    );
    let initial = call("GET", org, Some(&key.key), Value::Null, None).1;
    assert_eq!(initial["name"], "Work fixture");
    let payload = json!({"expected_revision":1,"name":"Empresa de prueba","address":"Private address","phone":"+503 0000 0000","country_code":"SV"});
    let changed = call(
        "PATCH",
        org,
        Some(&key.key),
        payload.clone(),
        Some("org-once"),
    );
    assert_eq!(changed.0, StatusCode::OK);
    assert_eq!(changed.1["revision"], 2);
    assert_eq!(
        call(
            "PATCH",
            org,
            Some(&key.key),
            payload.clone(),
            Some("org-once")
        )
        .1,
        changed.1
    );
    assert_eq!(
        call("PATCH", org, Some(&key.key), payload, Some("stale")).0,
        StatusCode::CONFLICT
    );
    let branding = call(
        "GET",
        "/api/v1/public/organization",
        None,
        Value::Null,
        None,
    )
    .1;
    assert_eq!(branding["name"], "Empresa de prueba");
    assert!(branding.get("phone").is_none());
    assert!(branding.get("address").is_none());
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(128, 128)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let logo = call(
        "PUT",
        "/api/v1/settings/organization/logo",
        Some(&key.key),
        json!({"expected_revision":2,"data_base64":base64::engine::general_purpose::STANDARD.encode(png.into_inner())}),
        Some("logo-once"),
    );
    assert_eq!(logo.0, StatusCode::OK);
    assert_eq!(logo.1["has_logo"], true);
    let bytes = f.store.organization_logo().unwrap();
    let logo = image::load_from_memory(&bytes).unwrap();
    assert_eq!((logo.width(), logo.height()), (512, 512));
    assert_eq!(call("PUT","/api/v1/settings/organization/logo",Some(&key.key),json!({"expected_revision":3,"data_base64":base64::engine::general_purpose::STANDARD.encode(b"<svg></svg>")}),Some("bad-logo")).0,StatusCode::UNPROCESSABLE_ENTITY);
    let openai = "/api/v1/settings/services/openai";
    let secret = zeroize::Zeroizing::new(SecretGenerator::token("fixture").unwrap());
    let mut settings = json!({"endpoint":"https://gateway.example.invalid/ai/v1","model":"company/model","embedding_model":"text-embedding-3-small","effort":"medium","enabled":true});
    let payload = json!({"settings":settings,"secret":secret.as_str(),"expected_revision":1});
    assert_eq!(
        call(
            "PUT",
            openai,
            Some(&limited.key),
            payload.clone(),
            Some("denied")
        )
        .0,
        StatusCode::FORBIDDEN
    );
    let saved = call(
        "PUT",
        openai,
        Some(&key.key),
        payload.clone(),
        Some("ai-once"),
    );
    assert_eq!(saved.0, StatusCode::OK);
    assert_eq!(saved.1["has_secret"], true);
    assert!(!saved.1.to_string().contains(secret.as_str()));
    assert_eq!(saved.1["revision"], 2);
    assert_eq!(
        call("PUT", openai, Some(&key.key), payload, Some("ai-once")).1,
        saved.1
    );
    let runtime = f
        .store
        .service_runtime_settings(InstallationService::Openai)
        .unwrap()
        .unwrap();
    assert!(runtime.secret.unwrap().as_slice() == secret.as_bytes());
    settings["endpoint"] = json!("https://other.example.invalid/v1");
    assert_eq!(
        call(
            "PUT",
            openai,
            Some(&key.key),
            json!({"settings":settings,"expected_revision":2}),
            Some("move-without-key")
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    settings["endpoint"] = json!("http://other.example.invalid/v1");
    assert_eq!(
        call(
            "PUT",
            openai,
            Some(&key.key),
            json!({"settings":settings,"secret":secret.as_str(),"expected_revision":2}),
            Some("http-key")
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let smtp = json!({"host":"smtp.example.invalid","port":587,"username":"mailer","sender_email":"nexora@example.invalid","sender_name":"Nexora","tls_mode":"starttls","enabled":false});
    let saved = call(
        "PUT",
        "/api/v1/settings/services/smtp",
        Some(&key.key),
        json!({"settings":smtp,"secret":secret.as_str(),"expected_revision":1}),
        Some("smtp-once"),
    );
    assert_eq!(saved.0, StatusCode::OK);
    assert!(!saved.1.to_string().contains(secret.as_str()));
    assert!(
        f.store
            .smtp_runtime_settings()
            .unwrap()
            .unwrap()
            .secret
            .unwrap()
            .as_slice()
            == secret.as_bytes()
    );
    let test = json!({"recipient":"delivery@example.invalid"});
    let job = call(
        "POST",
        "/api/v1/settings/smtp/tests",
        Some(&key.key),
        test.clone(),
        Some("test-once"),
    );
    assert_eq!(job.0, StatusCode::ACCEPTED);
    assert_eq!(
        call(
            "POST",
            "/api/v1/settings/smtp/tests",
            Some(&key.key),
            test,
            Some("test-once")
        )
        .1,
        job.1
    );
    let mail = f.store.claim_email_notification(false).unwrap().unwrap();
    assert_eq!(mail.kind, "test");
    assert_eq!(mail.email, "delivery@example.invalid");
    f.store
        .finish_email_notification(&mail.event_key, mail.attempts, "sent")
        .unwrap();
    let status = call(
        "GET",
        &format!(
            "/api/v1/settings/smtp/tests/{}",
            job.1["id"].as_str().unwrap()
        ),
        Some(&key.key),
        Value::Null,
        None,
    );
    assert_eq!(status.1["status"], "sent");
    assert_eq!(
        call(
            "GET",
            "/api/v1/settings/notification-recipients",
            Some(&limited.key),
            Value::Null,
            None
        )
        .0,
        StatusCode::FORBIDDEN
    );
}
