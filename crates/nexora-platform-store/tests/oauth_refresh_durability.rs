#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "../../nexora-source/tests/support/git_fixture.rs"]
mod git_fixture;
#[path = "support/github_source_fixture.rs"]
mod github_source_fixture;
#[path = "support/gitlab_source_fixture.rs"]
mod gitlab_source_fixture;
#[path = "../../nexora-source/tests/support/test_directory.rs"]
mod test_directory;
use nexora_platform::*;
use nexora_platform_store::{
    GitNetworkPolicy, PlatformStore, ProviderSecretContext, SecretGenerator,
};
use nexora_runtime::DatabaseConnections;
use serde_json::json;
use std::path::Path;
use zeroize::Zeroizing;

#[test]
#[ignore = "requires PostgreSQL and local generated GitHub App fixture"]
fn github_rotation_survives_identity_outage_and_mismatch_never_grants_access() {
    let git = git_fixture::GitFixture::new("github-refresh-durability");
    let commit = "1".repeat(40);
    let provider = github_source_fixture::GithubSourceFixture::start(&git, &commit);
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = SecretGenerator::id("nexora_platform_gh_rotation").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = Zeroizing::new(SecretGenerator::token("owner_password").unwrap());
    store
        .bootstrap("Rotation test", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let policy = GitNetworkPolicy {
        github_origin: provider.origin().into(),
        github_api_origin: provider.origin().into(),
        ..Default::default()
    };
    store
        .configure_github(
            &ctx,
            &provider.registration(&policy.callback_origin),
            &policy,
        )
        .unwrap();
    let mut sql = connect_sql(&config, &namespace);
    let connection = seed(
        &mut store,
        &mut sql,
        &ctx,
        GitProvider::Github,
        &policy.github_host().unwrap(),
        provider.token(),
    );
    provider.control(json!({"identity_failure":true}));
    assert!(matches!(
        store.connection_remote_repositories(&ctx, &connection, 1, 20, &policy),
        Err(PlatformError::Unavailable)
    ));
    assert_eq!(
        revision(&mut sql, &connection),
        2,
        "successful rotation must survive the later identity failure"
    );
    assert_eq!(
        std::fs::read_to_string(git.directory.path().join("github-control.refresh-count")).unwrap(),
        "1"
    );
    provider.control(json!({}));
    assert_eq!(
        store
            .connection_remote_repositories(&ctx, &connection, 1, 20, &policy)
            .unwrap()
            .items
            .len(),
        1
    );
    assert_eq!(revision(&mut sql, &connection), 2);
    assert_eq!(
        std::fs::read_to_string(git.directory.path().join("github-control.refresh-count")).unwrap(),
        "1"
    );
    provider.control(json!({"identity_mismatch":true}));
    assert!(matches!(
        store.connection_remote_repositories(&ctx, &connection, 1, 20, &policy),
        Err(PlatformError::Unauthorized)
    ));
    assert!(
        store
            .connection_remote_repositories(&ctx, &connection, 1, 20, &policy)
            .is_err()
    );
}

#[test]
#[ignore = "requires PostgreSQL and local generated GitLab fixture"]
fn gitlab_rotation_survives_identity_outage_and_mismatch_never_grants_access() {
    let git = git_fixture::GitFixture::new("gitlab-refresh-durability");
    let commit = "1".repeat(40);
    let provider = gitlab_source_fixture::GitlabSourceFixture::start(&git, &commit);
    let config =
        DatabaseConnections::load(Path::new("../../.nexora/local/connections.json")).unwrap();
    let namespace = SecretGenerator::id("nexora_platform_gl_rotation").unwrap();
    let mut store = PlatformStore::connect(&config, &namespace).unwrap();
    store.initialize().unwrap();
    let password = Zeroizing::new(SecretGenerator::token("owner_password").unwrap());
    store
        .bootstrap("Rotation test", "owner", &password, "Owner")
        .unwrap();
    let session = store.login("owner", &password).unwrap();
    let ctx = store.authenticate(&session.access_token).unwrap();
    let policy = GitNetworkPolicy {
        gitlab_origin: provider.origin().into(),
        ..Default::default()
    };
    store
        .configure_gitlab(
            &ctx,
            &GitlabClientRegistration {
                client_id: "fixture-client".into(),
                client_secret: SecretGenerator::token("client_secret").unwrap(),
                redirect_uri: format!("{}/api/v1/git/gitlab/callback", policy.callback_origin),
                enabled: true,
                expected_revision: 0,
            },
            &policy,
        )
        .unwrap();
    let mut sql = connect_sql(&config, &namespace);
    let connection = seed(
        &mut store,
        &mut sql,
        &ctx,
        GitProvider::Gitlab,
        &policy.host().unwrap(),
        provider.token(),
    );
    provider.control(json!({"identity_failure":true}));
    assert!(matches!(
        store.connection_remote_repositories(&ctx, &connection, 1, 20, &policy),
        Err(PlatformError::Unavailable)
    ));
    assert_eq!(
        revision(&mut sql, &connection),
        2,
        "successful rotation must survive the later identity failure"
    );
    assert_eq!(
        std::fs::read_to_string(git.directory.path().join("provider-control.refresh-count"))
            .unwrap(),
        "1"
    );
    provider.control(json!({}));
    assert!(
        store
            .connection_remote_repositories(&ctx, &connection, 1, 20, &policy)
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(revision(&mut sql, &connection), 2);
    assert_eq!(
        std::fs::read_to_string(git.directory.path().join("provider-control.refresh-count"))
            .unwrap(),
        "1"
    );
    provider.control(json!({"identity_mismatch":true}));
    assert!(matches!(
        store.connection_remote_repositories(&ctx, &connection, 1, 20, &policy),
        Err(PlatformError::Unauthorized)
    ));
    assert!(
        store
            .connection_remote_repositories(&ctx, &connection, 1, 20, &policy)
            .is_err()
    );
}
fn seed(
    store: &mut PlatformStore,
    sql: &mut postgres::Client,
    ctx: &AuthContext,
    provider: GitProvider,
    host: &str,
    token: &str,
) -> String {
    let id = SecretGenerator::id("conn").unwrap();
    let github = provider == GitProvider::Github;
    let value = json!({"access_token":token,"refresh_token":SecretGenerator::token("refresh").unwrap(),"token_type":"Bearer","scope":if github {""} else {"read_api read_repository"},"expires_in":3600,"refresh_token_expires_in":86400});
    let plain = Zeroizing::new(serde_json::to_vec(&value).unwrap());
    let envelope = store
        .provider_secret_cipher()
        .unwrap()
        .seal(
            &ProviderSecretContext {
                binding_id: id.clone(),
                provider: provider.into(),
                host: host.into(),
                purpose: if github {
                    "github_user_tokens"
                } else {
                    "gitlab_tokens"
                }
                .into(),
                revision: 1,
            },
            &plain,
        )
        .unwrap();
    let encrypted = serde_json::to_value(envelope).unwrap();
    let provider_name = if github { "github" } else { "gitlab" };
    sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at,installation_id) VALUES($1,$2,$3,$4,'42','fixture','active',1,$5,clock_timestamp()-interval '1 hour',$6)", &[&id,&provider_name,&host,&ctx.actor_id(),&encrypted,&if github {Some("7")} else {None}]).unwrap();
    id
}
fn revision(sql: &mut postgres::Client, id: &str) -> i64 {
    sql.query_one(
        "SELECT credential_revision FROM git_connections WHERE id=$1",
        &[&id],
    )
    .unwrap()
    .get(0)
}
fn connect_sql(config: &DatabaseConnections, namespace: &str) -> postgres::Client {
    let password = Zeroizing::new(
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{namespace}\""))
        .unwrap();
    sql
}
