#[path = "support/email_flow_fixture.rs"]
mod email_flow_fixture;
use email_flow_fixture::EmailFlowFixture;
use nexora_platform::*;
use sha2::{Digest, Sha256};
#[test]
#[ignore = "requires PostgreSQL; verified self-service email, delivery, single use, throttling and preferences"]
fn users_verify_their_own_email_before_it_becomes_a_recovery_address() {
    let mut f = EmailFlowFixture::new();
    let actor = f.work.ctx.actor_id().to_owned();
    f.sql.execute("UPDATE notification_recipients SET assignments=false,approvals=false WHERE actor_id=$1",&[&actor]).unwrap();
    f.work
        .store
        .request_own_email(&f.work.ctx, "new@example.invalid")
        .unwrap();
    assert_eq!(
        f.work.store.own_email(&f.work.ctx).unwrap()["email"],
        "owner@example.invalid"
    );
    let mail = f
        .work
        .store
        .claim_email_notification(true)
        .unwrap()
        .unwrap();
    assert_eq!(mail.kind, "email_verification");
    assert_eq!(mail.email, "new@example.invalid");
    let (_, body) = f
        .work
        .store
        .email_content_for_delivery(&mail, "Organization")
        .unwrap();
    let code = zeroize::Zeroizing::new(
        body.split("<strong>")
            .nth(1)
            .unwrap()
            .split("</strong>")
            .next()
            .unwrap()
            .to_owned(),
    );
    assert!(code.len() == 6 && code.bytes().all(|b| b.is_ascii_digit()));
    f.work
        .store
        .finish_email_notification(&mail.event_key, mail.attempts, "sent")
        .unwrap();
    let wrong = if code.as_str() == "000000" {
        "111111"
    } else {
        "000000"
    };
    assert!(f.work.store.verify_own_email(&f.work.ctx, wrong).is_err());
    f.work.store.verify_own_email(&f.work.ctx, &code).unwrap();
    assert_eq!(
        f.work.store.own_email(&f.work.ctx).unwrap()["email"],
        "new@example.invalid"
    );
    assert!(f.work.store.verify_own_email(&f.work.ctx, &code).is_err());
    let row = f
        .sql
        .query_one(
            "SELECT assignments,approvals FROM notification_recipients WHERE actor_id=$1",
            &[&actor],
        )
        .unwrap();
    assert!(!row.get::<_, bool>(0));
    assert!(!row.get::<_, bool>(1));
    let notice = f
        .work
        .store
        .claim_email_notification(true)
        .unwrap()
        .unwrap();
    assert_eq!(notice.kind, "email_changed");
    assert_eq!(notice.email, "owner@example.invalid");
    assert!(
        f.work
            .store
            .email_content_for_delivery(&notice, "Organization")
            .unwrap()
            .1
            .contains("new@example.invalid")
    );
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            &actor,
            "No self email through MCP",
            KeyScope {
                capabilities: vec![Capability::ActorsManage],
                project_ids: vec![],
                all_projects: false,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let api = f.work.store.authenticate(&key.key).unwrap();
    assert!(
        f.work
            .store
            .request_own_email(&api, "another@example.invalid")
            .is_err()
    );
    for email in ["second@example.invalid", "third@example.invalid"] {
        f.work.store.request_own_email(&f.work.ctx, email).unwrap();
    }
    assert!(
        f.work
            .store
            .request_own_email(&f.work.ctx, "fourth@example.invalid")
            .is_err()
    );
    let row = f
        .sql
        .query_one(
            "SELECT id,digest FROM user_email_changes WHERE actor_id=$1 AND used_at IS NULL",
            &[&actor],
        )
        .unwrap();
    let id: String = row.get(0);
    let digest: Vec<u8> = row.get(1);
    let wrong = if digest == Sha256::digest(format!("{id}:000000").as_bytes()).to_vec() {
        "111111"
    } else {
        "000000"
    };
    for _ in 0..5 {
        assert!(f.work.store.verify_own_email(&f.work.ctx, wrong).is_err());
    }
    let count: i32 = f
        .sql
        .query_one(
            "SELECT attempts FROM user_email_changes WHERE id=$1",
            &[&id],
        )
        .unwrap()
        .get(0);
    assert_eq!(count, 5);
}
