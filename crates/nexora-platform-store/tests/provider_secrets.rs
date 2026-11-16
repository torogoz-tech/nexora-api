use nexora_platform::GitProvider;
use nexora_platform_store::{
    PlatformStore, ProviderSecretContext, ProviderSecretEnvelope, SecretGenerator,
};
use nexora_runtime::DatabaseConnections;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
#[test]
#[ignore = "requires PostgreSQL; provider envelope survives reload and rejects mismatched installation key"]
fn provider_envelope_uses_the_database_bound_installation_key_without_exposing_plaintext() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_provider_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let context = ProviderSecretContext {
        binding_id: "conn_fixture".into(),
        provider: GitProvider::Gitlab.into(),
        host: "gitlab.com".into(),
        purpose: "refresh_token".into(),
        revision: 1,
    };
    let secret = zeroize::Zeroizing::new(SecretGenerator::token("provider_test").unwrap());
    let encrypted = store
        .provider_secret_cipher()
        .unwrap()
        .seal(&context, secret.as_bytes())
        .unwrap();
    let wire = serde_json::to_vec(&encrypted).unwrap();
    assert!(!wire.windows(secret.len()).any(|w| w == secret.as_bytes()));
    drop(store);
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    let encrypted: ProviderSecretEnvelope = serde_json::from_slice(&wire).unwrap();
    let decoded = store
        .provider_secret_cipher()
        .unwrap()
        .open(&context, &encrypted)
        .unwrap();
    assert!(decoded.as_slice() == secret.as_bytes());
    let password =
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap();
    let mut client = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    client
        .execute(
            &format!("UPDATE \"{namespace}\".platform_crypto_state SET key_id=$1"),
            &[&"mismatched-fixture-key"],
        )
        .unwrap();
    assert!(store.provider_secret_cipher().is_err());
}
