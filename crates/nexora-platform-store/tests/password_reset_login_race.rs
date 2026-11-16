#[path = "support/email_flow_fixture.rs"]
mod email_flow_fixture;
use email_flow_fixture::EmailFlowFixture;
use nexora_platform::{NotificationRecipientChange, PlatformError};
use nexora_platform_store::{PlatformStore, SecretGenerator};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires PostgreSQL; a real reset races a login verified against the old credential"]
fn login_cannot_create_a_session_after_the_verified_password_was_reset() {
    let mut f = EmailFlowFixture::new();
    let old = Zeroizing::new(SecretGenerator::token("old_password").unwrap());
    let new = Zeroizing::new(SecretGenerator::token("new_password").unwrap());
    let actor = f
        .work
        .store
        .create_user(&f.work.ctx, "race_user", &old, "Race user")
        .unwrap();
    f.work
        .store
        .change_notification_recipient(
            &f.work.ctx,
            &actor.id,
            &NotificationRecipientChange {
                email: "race@example.invalid".into(),
                assignments: true,
                approvals: true,
                expected_revision: 0,
            },
        )
        .unwrap();
    let before = f.work.store.login("race_user", &old).unwrap();
    assert!(f.work.store.authenticate(&before.access_token).is_ok());
    f.work
        .store
        .request_password_recovery("race@example.invalid")
        .unwrap();
    let (_, token) = f.claim_token("password_reset");
    let key: i64 = f
        .sql
        .query_one("SELECT hashtextextended($1,0)", &[&f.work.namespace])
        .unwrap()
        .get(0);
    let blocker_pid: i32 = f
        .sql
        .query_one("SELECT pg_backend_pid()", &[])
        .unwrap()
        .get(0);
    f.sql
        .query_one("SELECT pg_advisory_lock($1)", &[&key])
        .unwrap();
    f.sql.batch_execute(&format!("CREATE FUNCTION pause_password_reset() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.actor_id='{}' THEN PERFORM pg_advisory_xact_lock({key}::bigint); END IF; RETURN NEW; END $$; CREATE TRIGGER pause_password_reset AFTER UPDATE OF password_hash ON users FOR EACH ROW EXECUTE FUNCTION pause_password_reset()", actor.id)).unwrap();
    let config = f.work.config.clone();
    let namespace = f.work.namespace.clone();
    let replacement = new.to_string();
    let reset = std::thread::spawn(move || {
        let mut store = PlatformStore::connect(&config, &namespace).unwrap();
        store.complete_password_recovery(&token, &replacement)
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let reset_pid: i32 = loop {
        if let Some(row) = f.sql.query_opt("SELECT pid FROM pg_stat_activity WHERE wait_event='advisory' AND $1=ANY(pg_blocking_pids(pid))", &[&blocker_pid]).unwrap() {
            break row.get(0);
        }
        assert!(
            Instant::now() < deadline,
            "reset did not reach the credential update barrier"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    let config = f.work.config.clone();
    let namespace = f.work.namespace.clone();
    let password = old.to_string();
    let login = std::thread::spawn(move || {
        let mut store = PlatformStore::connect(&config, &namespace).unwrap();
        store.login("race_user", &password)
    });
    loop {
        let waiting: bool = f
            .sql
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
                &[&reset_pid],
            )
            .unwrap()
            .get(0);
        if waiting {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "login did not reach the concurrent credential/session boundary"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    f.sql
        .query_one("SELECT pg_advisory_unlock($1)", &[&key])
        .unwrap();
    reset.join().unwrap().unwrap();
    assert!(matches!(
        login.join().unwrap(),
        Err(PlatformError::Unauthorized)
    ));
    assert!(f.work.store.authenticate(&before.access_token).is_err());
    assert!(f.work.store.login("race_user", &old).is_err());
    let after = f.work.store.login("race_user", &new).unwrap();
    assert!(f.work.store.authenticate(&after.access_token).is_ok());
    let active: i64 = f
        .sql
        .query_one(
            "SELECT count(*) FROM sessions WHERE actor_id=$1 AND revoked_at IS NULL",
            &[&actor.id],
        )
        .unwrap()
        .get(0);
    assert_eq!(active, 1);
}
