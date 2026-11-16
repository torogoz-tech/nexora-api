use nexora_platform::{
    AuthenticationPolicyChange, OidcIdentityAssignment, OidcProviderRegistration,
};
use nexora_platform_store::{OidcNetworkPolicy, PlatformStore, SecretGenerator};
use nexora_runtime::DatabaseConnections;
use std::{
    path::Path,
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[test]
#[ignore = "requires PostgreSQL; gate a native session insert while switching to corporate-only login"]
fn corporate_only_policy_cannot_leave_a_concurrently_created_local_session_active() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_authrace_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let password = SecretGenerator::token("test_password").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    store
        .bootstrap("Authentication race", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let provider = store
        .configure_oidc(
            &ctx,
            &OidcProviderRegistration {
                name: "Configured corporate IdP".into(),
                issuer: "https://accounts.google.com".into(),
                client_id: "fixture-client".into(),
                client_secret: SecretGenerator::token("fixture_client_secret").unwrap(),
                redirect_uri: "http://127.0.0.1:18080/api/v1/auth/oidc/callback".into(),
                enabled: true,
                expected_revision: 0,
            },
            &OidcNetworkPolicy::default(),
        )
        .unwrap();
    store
        .assign_oidc_identity(
            &ctx,
            ctx.actor_id(),
            &OidcIdentityAssignment {
                provider_id: provider.id,
                subject: "corporate-owner".into(),
            },
        )
        .unwrap();
    let pg_password =
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap();
    let mut control = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(pg_password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    let gate = format!("{namespace}:local-session-gate");
    control.batch_execute(&format!("CREATE FUNCTION \"{namespace}\".session_gate() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(hashtextextended('{gate}',0)); RETURN NEW; END $$; CREATE TRIGGER gated_native_session AFTER INSERT ON \"{namespace}\".sessions FOR EACH ROW WHEN (NEW.authentication_provider_id IS NULL) EXECUTE FUNCTION \"{namespace}\".session_gate()")).unwrap();
    control
        .query_one("SELECT pg_advisory_lock(hashtextextended($1,0))", &[&gate])
        .unwrap();
    let c = config.clone();
    let n = namespace.clone();
    let p = password.clone();
    let login =
        std::thread::spawn(move || PlatformStore::connect(&c, &n).unwrap().login("owner", &p));
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut gated = false;
    while Instant::now() < deadline {
        gated=control.query_one("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND NOT granted AND classid=((hashtextextended($1,0)>>32)&4294967295)::oid AND objid=(hashtextextended($1,0)&4294967295)::oid)",&[&gate]).unwrap().get(0);
        if gated {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let (done, receiver) = mpsc::channel();
    let c = config.clone();
    let n = namespace.clone();
    let switch = std::thread::spawn(move || {
        let outcome = PlatformStore::connect(&c, &n)
            .unwrap()
            .change_authentication_policy(
                &ctx,
                &AuthenticationPolicyChange {
                    local_enabled: false,
                    expected_revision: 1,
                },
            );
        done.send(outcome).unwrap();
    });
    let early = receiver.recv_timeout(Duration::from_secs(1));
    let committed_before_login = early.is_ok();
    control
        .query_one(
            "SELECT pg_advisory_unlock(hashtextextended($1,0))",
            &[&gate],
        )
        .unwrap();
    let issued = login.join().unwrap().unwrap();
    let switched = match early {
        Ok(value) => {
            switch.join().unwrap();
            value
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let value = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
            switch.join().unwrap();
            value
        }
        Err(error) => panic!("Policy worker failed: {error}"),
    };
    assert!(gated);
    assert!(
        !committed_before_login,
        "Policy committed before the gated native login; its new session escapes revocation"
    );
    assert!(switched.is_ok());
    assert!(store.authenticate(&issued.access_token).is_err());
}
