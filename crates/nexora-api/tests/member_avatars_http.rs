#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use axum::{
    body::Body,
    http::{HeaderMap, Request, StatusCode},
};
use base64::Engine;
use http_body_util::BodyExt;
use nexora_api::{ApiServer, ApiState};
use nexora_platform::*;
use serde_json::{Value, json};
use tower::ServiceExt;
use work_item_fixture::WorkItemFixture;

#[test]
#[ignore = "requires PostgreSQL; own session photo normalization, idempotency and privacy"]
fn member_avatars_require_own_human_session_and_return_private_normalized_png() {
    let mut f = WorkItemFixture::new();
    let password = zeroize::Zeroizing::new(
        nexora_platform_store::SecretGenerator::token("fixture_password").unwrap(),
    );
    let user = f
        .store
        .create_user(&f.ctx, "photographer", &password, "Profile photographer")
        .unwrap();
    let session = f.store.login("photographer", &password).unwrap();
    let admin_key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "Avatar reads",
            KeyScope {
                capabilities: Capability::all(),
                project_ids: vec![],
                all_projects: true,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let executor = tokio::runtime::Runtime::new().unwrap();
    let app = executor.block_on(async {
        ApiServer::router(
            ApiState::new(f.config.clone(), f.namespace.clone(), 4)
                .await
                .unwrap(),
        )
    });
    let request =
        |method: &str, path: &str, token: &str, payload: Option<Value>, id: Option<&str>| {
            executor.block_on(async {
                let mut builder = Request::builder()
                    .method(method)
                    .uri(path)
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json");
                if let Some(id) = id {
                    builder = builder.header("idempotency-key", id);
                }
                let response = app
                    .clone()
                    .oneshot(
                        builder
                            .body(Body::from(
                                payload
                                    .map(|value| serde_json::to_vec(&value).unwrap())
                                    .unwrap_or_default(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let status = response.status();
                let headers: HeaderMap = response.headers().clone();
                let bytes = response.into_body().collect().await.unwrap().to_bytes();
                (status, headers, bytes)
            })
        };
    let mut source = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(64, 32)
        .write_to(&mut source, image::ImageFormat::Jpeg)
        .unwrap();
    let source = source.into_inner();
    // JPEG EXIF orientation 6 rotates the 64x32 source clockwise before normalization.
    let exif = b"Exif\0\0II*\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
    let mut oriented = source[..2].to_vec();
    oriented.extend_from_slice(b"\xff\xe1");
    oriented.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    oriented.extend_from_slice(exif);
    oriented.extend_from_slice(&source[2..]);
    let input = json!({"image_base64":base64::engine::general_purpose::STANDARD.encode(oriented)});
    let avatar_path = format!("/api/v1/members/{}/avatar", user.id);
    assert_eq!(
        request("GET", &avatar_path, &session.access_token, None, None).0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            "PUT",
            "/api/v1/me/avatar",
            &session.access_token,
            Some(input.clone()),
            None
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, _, bytes) = request(
        "PUT",
        "/api/v1/me/avatar",
        &session.access_token,
        Some(input.clone()),
        Some("avatar-create"),
    );
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap(),
        json!({"avatar_revision":1})
    );
    let replay = request(
        "PUT",
        "/api/v1/me/avatar",
        &session.access_token,
        Some(input.clone()),
        Some("avatar-create"),
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&replay.2).unwrap(),
        json!({"avatar_revision":1})
    );
    let (status, headers, bytes) = request("GET", &avatar_path, &session.access_token, None, None);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "image/png");
    assert_eq!(headers["cache-control"], "private, no-store");
    let normalized = image::load_from_memory(&bytes).unwrap();
    assert_eq!((normalized.width(), normalized.height()), (512, 512));
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    let pixels = normalized.to_rgba8();
    assert_eq!(pixels.get_pixel(256, 0).0[3], 255);
    assert_eq!(pixels.get_pixel(0, 256).0[3], 0);
    assert!(!bytes.windows(4).any(|chunk| chunk == b"eXIf"));
    assert_eq!(
        request("GET", &avatar_path, &admin_key.key, None, None).0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            "GET",
            &format!("/api/v1/members/{}/avatar", f.ctx.actor_id()),
            &session.access_token,
            None,
            None
        )
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            "PUT",
            "/api/v1/me/avatar",
            &admin_key.key,
            Some(input.clone()),
            Some("avatar-agent-denied")
        )
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            "DELETE",
            "/api/v1/me/avatar",
            &admin_key.key,
            None,
            Some("avatar-agent-delete-denied")
        )
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            "PUT",
            "/api/v1/me/avatar",
            &session.access_token,
            Some(json!({"image_base64":"not-an-image"})),
            Some("avatar-malformed")
        )
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, _, bytes) = request(
        "PUT",
        "/api/v1/me/avatar",
        &session.access_token,
        Some(input.clone()),
        Some("avatar-replace"),
    );
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap(),
        json!({"avatar_revision":2})
    );
    let profile = request(
        "GET",
        &format!("/api/v1/members/{}/profile", user.id),
        &session.access_token,
        None,
        None,
    );
    let profile: Value = serde_json::from_slice(&profile.2).unwrap();
    assert_eq!(profile["member"]["avatar_revision"], 2);
    assert!(profile["member"].get("image_png").is_none());
    for _ in 0..2 {
        let (status, _, bytes) = request(
            "DELETE",
            "/api/v1/me/avatar",
            &session.access_token,
            None,
            Some("avatar-delete"),
        );
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            json!({"avatar_revision":3})
        );
    }
    assert_eq!(
        request("GET", &avatar_path, &session.access_token, None, None).0,
        StatusCode::NOT_FOUND
    );
}
