#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::{Value, json};
use tower::ServiceExt;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; email template HTTP contract and authorization; no external calls"]
fn member_email_http_connects_invitation_and_recovery_without_sending_mail() {
    let mut f = WorkItemFixture::new();
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Settings fixture",
            KeyScope {
                capabilities: vec![Capability::InstallationManage, Capability::ActorsManage],
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
            (
                status,
                serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            )
        })
    };
    let settings = "/api/v1/settings/email-access";
    assert_eq!(
        call("GET", settings, None, Value::Null, None).0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            "GET",
            "/api/v1/settings/members",
            Some(&limited.key),
            Value::Null,
            None
        )
        .0,
        StatusCode::FORBIDDEN
    );
    let (_, current) = call("GET", settings, Some(&key.key), Value::Null, None);
    assert_eq!(call("PUT",settings,Some(&key.key),json!({"public_web_url":"https://example.com/unsafe?redirect=x","expected_revision":current["revision"]}),Some("bad-web")).0,StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(call("PUT",settings,Some(&key.key),json!({"public_web_url":"https://nexora.example","expected_revision":current["revision"]}),Some("web-url")).0,StatusCode::OK);
    let (_, smtp) = call(
        "GET",
        "/api/v1/settings/services/smtp",
        Some(&key.key),
        Value::Null,
        None,
    );
    assert_eq!(call("PUT","/api/v1/settings/services/smtp",Some(&key.key),json!({"expected_revision":smtp["revision"],"settings":{"enabled":true,"host":"smtp.example.invalid","port":587,"username":"","sender_email":"noreply@example.invalid","sender_name":"Test","tls_mode":"starttls"}}),Some("enable-smtp")).0,StatusCode::OK);
    let owner = f.ctx.actor_id().to_owned();
    f.store
        .change_notification_recipient(
            &f.ctx,
            &owner,
            &NotificationRecipientChange {
                email: "owner@example.invalid".into(),
                assignments: true,
                approvals: true,
                expected_revision: 0,
            },
        )
        .unwrap();
    let invite = json!({"name":"Invited","email":"invited@example.invalid"});
    assert_eq!(
        call(
            "POST",
            "/api/v1/settings/invitations",
            Some(&limited.key),
            invite.clone(),
            Some("forbidden-invite")
        )
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, created) = call(
        "POST",
        "/api/v1/settings/invitations",
        Some(&key.key),
        invite.clone(),
        Some("invite-member"),
    );
    assert_eq!(status, StatusCode::CREATED);
    assert!(!created.to_string().contains("nxi_"));
    assert_eq!(
        call(
            "POST",
            "/api/v1/settings/invitations",
            Some(&key.key),
            invite,
            Some("invite-member")
        )
        .1,
        created
    );
    let mail = f.store.claim_email_notification(true).unwrap().unwrap();
    assert_eq!(mail.kind, "invitation");
    let (_, body) = f.store.email_content_for_delivery(&mail, "Org").unwrap();
    let token = zeroize::Zeroizing::new(
        body.split("#token=")
            .nth(1)
            .unwrap()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect::<String>(),
    );
    let (status, details) = call(
        "POST",
        "/api/v1/auth/invitations/inspect",
        None,
        json!({"token":token.as_str()}),
        None,
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(details["email"], "invited@example.invalid");
    let password = zeroize::Zeroizing::new(
        nexora_platform_store::SecretGenerator::token("initial_password").unwrap(),
    );
    let accept = json!({"token":token.as_str(),"name":"Invited","username":"invited","password":password.as_str()});
    assert_eq!(
        call(
            "POST",
            "/api/v1/auth/invitations/accept",
            None,
            accept.clone(),
            None
        )
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        call(
            "POST",
            "/api/v1/auth/invitations/accept",
            None,
            accept,
            None
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let session = f.store.login("invited", &password).unwrap();
    let notice = f.store.claim_email_notification(true).unwrap().unwrap();
    assert_eq!(notice.kind, "invitation_accepted");
    f.store
        .finish_email_notification(&notice.event_key, notice.attempts, "sent")
        .unwrap();
    let known = call(
        "POST",
        "/api/v1/auth/password/request",
        None,
        json!({"email":"invited@example.invalid"}),
        None,
    );
    let unknown = call(
        "POST",
        "/api/v1/auth/password/request",
        None,
        json!({"email":"not-registered@example.invalid"}),
        None,
    );
    assert_eq!(known, unknown);
    assert_eq!(known.0, StatusCode::ACCEPTED);
    let recovery = f.store.claim_email_notification(true).unwrap().unwrap();
    assert_eq!(recovery.kind, "password_reset");
    let (_, body) = f
        .store
        .email_content_for_delivery(&recovery, "Org")
        .unwrap();
    let token = zeroize::Zeroizing::new(
        body.split("#token=")
            .nth(1)
            .unwrap()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect::<String>(),
    );
    let next = zeroize::Zeroizing::new(
        nexora_platform_store::SecretGenerator::token("next_password").unwrap(),
    );
    assert_eq!(
        call(
            "POST",
            "/api/v1/auth/password/complete",
            None,
            json!({"token":token.as_str(),"password":next.as_str()}),
            None
        )
        .0,
        StatusCode::NO_CONTENT
    );
    assert!(f.store.login("invited", &password).is_err());
    assert!(f.store.login("invited", &next).is_ok());
    assert!(f.store.authenticate(&session.access_token).is_err());
}
