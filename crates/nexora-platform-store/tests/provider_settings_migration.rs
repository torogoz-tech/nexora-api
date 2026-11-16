use nexora_platform::{GitConfigurationStateChange, GitProvider, PlatformError};
use nexora_platform_store::{
    GitNetworkPolicy, PlatformStore, ProviderSecretContext, SecretGenerator,
};
use nexora_runtime::DatabaseConnections;
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local PostgreSQL; representative schema8 encrypted provider settings"]
fn legacy_settings_keep_ciphertext_and_credential_context_when_administrative_revision_changes() {
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = format!(
        "nexora_platform_settings_migration_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    store
        .bootstrap("Settings fixture", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let pg_password = Zeroizing::new(
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut client = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(pg_password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    client
        .batch_execute(&format!("SET search_path TO \"{namespace}\""))
        .unwrap();
    let exists: bool = client.query_one("SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=$1 AND table_name='github_settings' AND column_name='credential_revision')", &[&namespace]).unwrap().get(0);
    assert!(exists, "Provider credential revision migration is missing");
    let github_secret = Zeroizing::new(
        serde_json::to_vec(&json!({
            "client_secret":SecretGenerator::token("fixture_secret").unwrap(),
            "private_key_pem":SecretGenerator::token("fixture_cipher_payload").unwrap(),
            "webhook_secret":SecretGenerator::token("fixture_hook").unwrap()
        }))
        .unwrap(),
    );
    let gitlab_secret = Zeroizing::new(SecretGenerator::token("fixture_secret").unwrap());
    let cipher = store.provider_secret_cipher().unwrap();
    let github = serde_json::to_value(
        cipher
            .seal(
                &ProviderSecretContext {
                    binding_id: "github-settings".into(),
                    provider: GitProvider::Github.into(),
                    host: "github.com".into(),
                    purpose: "github_app_secrets".into(),
                    revision: 5,
                },
                &github_secret,
            )
            .unwrap(),
    )
    .unwrap();
    let gitlab = serde_json::to_value(
        cipher
            .seal(
                &ProviderSecretContext {
                    binding_id: "gitlab-settings".into(),
                    provider: GitProvider::Gitlab.into(),
                    host: "gitlab.com".into(),
                    purpose: "gitlab_client_secret".into(),
                    revision: 5,
                },
                gitlab_secret.as_bytes(),
            )
            .unwrap(),
    )
    .unwrap();
    // Reconstruct only this generated fixture's schema8, preserving real v1 envelopes at revision5.
    client.batch_execute("ALTER TABLE github_settings DROP COLUMN credential_revision; ALTER TABLE gitlab_settings DROP COLUMN credential_revision; DELETE FROM platform_schema_versions WHERE version=9").unwrap();
    client.execute("INSERT INTO github_settings(singleton,app_id,client_id,slug,origin,api_origin,host,redirect_uri,enabled,revision,secrets) VALUES(true,'88','fixture-client','fixture','https://github.com','https://api.github.com','github.com','http://127.0.0.1:18080/api/v1/git/github/callback',true,5,$1)", &[&github]).unwrap();
    client.execute("INSERT INTO gitlab_settings(singleton,origin,host,client_id,client_secret,redirect_uri,enabled,revision) VALUES(true,'https://gitlab.com','gitlab.com','fixture-client',$1,'http://127.0.0.1:18080/api/v1/git/gitlab/callback',true,5)", &[&gitlab]).unwrap();
    store.initialize().unwrap();
    let row = client
        .query_one(
            "SELECT secrets,credential_revision FROM github_settings",
            &[],
        )
        .unwrap();
    assert!(
        row.get::<_, Value>(0) == github,
        "Migration rewrote GitHub ciphertext"
    );
    assert_eq!(row.get::<_, i64>(1), 5);
    let row = client
        .query_one(
            "SELECT client_secret,credential_revision FROM gitlab_settings",
            &[],
        )
        .unwrap();
    assert!(
        row.get::<_, Value>(0) == gitlab,
        "Migration rewrote GitLab ciphertext"
    );
    assert_eq!(row.get::<_, i64>(1), 5);
    client
        .batch_execute(
            "UPDATE github_settings SET revision=7; UPDATE gitlab_settings SET revision=7",
        )
        .unwrap();
    let policy = GitNetworkPolicy::default();
    assert!(
        store.start_github(&ctx, &policy).is_ok(),
        "GitHub secret followed administrative instead of credential revision"
    );
    assert!(
        store.start_gitlab(&ctx, &policy).is_ok(),
        "GitLab secret followed administrative instead of credential revision"
    );
    let mut changed_policy = policy.clone();
    changed_policy.callback_origin = "http://127.0.0.1:18081".into();
    let unchanged = GitConfigurationStateChange {
        enabled: true,
        expected_revision: 7,
    };
    assert!(
        matches!(
            store.change_github_configuration_state(&ctx, &unchanged, &changed_policy),
            Err(PlatformError::Conflict(_))
        ),
        "GitHub enabled with stale operator callback"
    );
    assert!(
        matches!(
            store.change_gitlab_configuration_state(&ctx, &unchanged, &changed_policy),
            Err(PlatformError::Conflict(_))
        ),
        "GitLab enabled with stale operator callback"
    );
    let disable = GitConfigurationStateChange {
        enabled: false,
        expected_revision: 7,
    };
    assert!(
        store
            .change_github_configuration_state(&ctx, &disable, &changed_policy)
            .is_ok(),
        "Changed callback prevented local GitHub disable"
    );
    assert!(
        store
            .change_gitlab_configuration_state(&ctx, &disable, &changed_policy)
            .is_ok(),
        "Changed callback prevented local GitLab disable"
    );
    let enable = GitConfigurationStateChange {
        enabled: true,
        expected_revision: 8,
    };
    assert!(
        store
            .change_github_configuration_state(&ctx, &enable, &policy)
            .is_ok()
    );
    assert!(
        store
            .change_gitlab_configuration_state(&ctx, &enable, &policy)
            .is_ok()
    );
    assert!(store.start_github(&ctx, &policy).is_ok());
    assert!(store.start_gitlab(&ctx, &policy).is_ok());
}
