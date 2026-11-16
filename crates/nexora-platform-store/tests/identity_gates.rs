use hmac::{Hmac, Mac};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use sha1::Sha1;
use std::path::Path;
#[test]
#[ignore = "requires PostgreSQL; verifies first factor isolation, five skips, enrollment, replay and exemptions"]
fn identity_gates_enforce_five_skips_and_totp() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let ns = SecretGenerator::id("nexora_platform_gate").unwrap();
    let mut store = PlatformStore::connect(&config, &ns).unwrap();
    store.initialize().unwrap();
    let password = SecretGenerator::token("test_password").unwrap();
    store
        .bootstrap("Gate test", "owner", &password, "Owner")
        .unwrap();
    for remaining in (0..5).rev() {
        let challenge = store.login("owner", &password).unwrap();
        assert_eq!(challenge.next_step.as_deref(), Some("totp_enroll"));
        assert!(store.authenticate(&challenge.access_token).is_err());
        let status = store
            .identity_gate(&challenge.access_token, "status", "")
            .unwrap();
        assert_eq!(status["remaining_skips"], remaining + 1);
        let session = store
            .identity_gate(&challenge.access_token, "skip", "")
            .unwrap();
        assert!(
            store
                .authenticate(session["access_token"].as_str().unwrap())
                .is_ok()
        );
        assert!(store.authenticate(&challenge.access_token).is_err());
        assert!(
            store
                .identity_gate(&challenge.access_token, "skip", "")
                .is_err()
        );
    }
    let challenge = store.login("owner", &password).unwrap();
    assert!(
        store
            .identity_gate(&challenge.access_token, "skip", "")
            .is_err()
    );
    let setup = store
        .identity_gate(&challenge.access_token, "setup", "")
        .unwrap();
    let encoded = setup["secret"].as_str().unwrap();
    let mut seed = Vec::new();
    let mut bits = 0u32;
    let mut count = 0;
    for b in encoded.bytes() {
        let n = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
            .iter()
            .position(|x| *x == b)
            .unwrap() as u32;
        bits = (bits << 5) | n;
        count += 5;
        if count >= 8 {
            count -= 8;
            seed.push((bits >> count) as u8);
        }
    }
    let step = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 30) as i64;
    let mut mac = Hmac::<Sha1>::new_from_slice(&seed).unwrap();
    mac.update(&(step as u64).to_be_bytes());
    let bytes = mac.finalize().into_bytes();
    let offset = (bytes[19] & 15) as usize;
    let n = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) & 0x7fffffff;
    let code = format!("{:06}", n % 1_000_000);
    let session = store
        .identity_gate(&challenge.access_token, "verify", &code)
        .unwrap();
    let ctx = store
        .authenticate(session["access_token"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        store.member_totp(&ctx, ctx.actor_id()).unwrap()["enrolled"],
        true
    );
    let again = store.login("owner", &password).unwrap();
    assert_eq!(again.next_step.as_deref(), Some("totp_verify"));
    assert!(store.authenticate(&again.access_token).is_err());
    assert!(
        store
            .identity_gate(&again.access_token, "skip", "")
            .is_err()
    );
    assert!(
        store
            .identity_gate(&again.access_token, "verify", &code)
            .is_err()
    );
    let user = store
        .create_user(&ctx, "newuser", &password, "New user")
        .unwrap();
    store.set_totp_exemption(&ctx, &user.id, true).unwrap();
    let exempt = store.login("newuser", &password).unwrap();
    assert!(exempt.next_step.is_none());
    let userctx = store.authenticate(&exempt.access_token).unwrap();
    assert!(store.member_totp(&userctx, &user.id).is_ok());
    assert!(store.set_totp_exemption(&userctx, &user.id, true).is_err());
    store.set_totp_exemption(&ctx, &user.id, false).unwrap();
    assert!(store.authenticate(&exempt.access_token).is_err());
    assert_eq!(
        store
            .login("newuser", &password)
            .unwrap()
            .next_step
            .as_deref(),
        Some("totp_enroll")
    );
    let policy = nexora_platform_store::OidcNetworkPolicy::default();
    assert!(!store.can_disable_local_login(&ctx).unwrap());
    assert!(
        store
            .change_authentication_policy(
                &ctx,
                &nexora_platform::AuthenticationPolicyChange {
                    expected_revision: 1,
                    local_enabled: false
                }
            )
            .is_err()
    );
    store
        .change_email_access_settings(
            &ctx,
            &nexora_platform::EmailAccessChange {
                expected_revision: 1,
                public_web_url: "https://nexora.example.invalid".into(),
            },
        )
        .unwrap();
    let effective = store.installation_oidc_policy(&policy).unwrap();
    assert_eq!(
        effective.allowed_callback_origins,
        vec!["https://nexora.example.invalid"]
    );
    assert!(
        effective
            .callback_url("https://nexora.example.invalid/api/v1/auth/oidc/callback")
            .is_ok()
    );
    assert!(
        effective
            .callback_url("http://127.0.0.1:18080/api/v1/auth/oidc/callback")
            .is_err()
    );
    assert!(effective.provider_url("https://unknown.invalid").is_err());
    // An OIDC pending account stays blocked even if exempt from TOTP, until a role is granted.
    let mut sql = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(
            std::fs::read_to_string(config.secrets_directory.join("postgres_password"))
                .unwrap()
                .trim(),
        )
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO {ns}"))
        .unwrap();
    sql.execute(
        "UPDATE user_security SET pending_role=true,totp_exempt=true WHERE actor_id=$1",
        &[&user.id],
    )
    .unwrap();
    let pending = store.login("newuser", &password).unwrap();
    assert_eq!(pending.next_step.as_deref(), Some("pending_role"));
    assert!(
        store
            .identity_gate(&pending.access_token, "skip", "")
            .is_err()
    );
    sql.execute(
        "INSERT INTO actor_roles(actor_id,role,all_projects) VALUES($1,'developer',true)",
        &[&user.id],
    )
    .unwrap();
    let allowed = store
        .identity_gate(&pending.access_token, "status", "")
        .unwrap();
    assert_eq!(allowed["stage"], "ready");
    assert!(
        store
            .authenticate(allowed["access_token"].as_str().unwrap())
            .is_ok()
    );
    sql.batch_execute(&format!("DROP SCHEMA {ns} CASCADE"))
        .unwrap();
}
