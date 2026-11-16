#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "../../nexora-source/tests/support/git_fixture.rs"]
mod git_fixture;
#[path = "support/gitlab_source_fixture.rs"]
mod gitlab_source_fixture;
#[path = "../../nexora-source/tests/support/test_directory.rs"]
mod test_directory;
#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use nexora_platform_store::{
    CodeReviewStore, GitNetworkPolicy, ProviderSecretContext, SecretGenerator,
};
use serde_json::json;
use zeroize::Zeroizing;
#[test]
#[ignore = "requires PostgreSQL and local generated GitLab fixture"]
fn review_claims_are_idempotent_pinned_authorized_and_independent_of_snapshots() {
    let git = git_fixture::GitFixture::new("manual-review");
    let provider = gitlab_source_fixture::GitlabSourceFixture::start(&git, &"a".repeat(40));
    let f = work_item_fixture::WorkItemFixture::new();
    let (mut store, config, namespace, ctx, project) =
        (f.store, f.config, f.namespace, f.ctx, f.project);
    let policy = GitNetworkPolicy {
        gitlab_origin: provider.origin().into(),
        ..Default::default()
    };
    let registration = GitlabClientRegistration {
        client_id: "fixture-client".into(),
        client_secret: SecretGenerator::token("fixture_client").unwrap(),
        redirect_uri: format!("{}/api/v1/git/gitlab/callback", policy.callback_origin),
        enabled: true,
        expected_revision: 0,
    };
    store
        .configure_gitlab(&ctx, &registration, &policy)
        .unwrap();
    let repository = store
        .register_repository(
            &ctx,
            &project.id,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "fixture/source".into(),
            },
        )
        .unwrap();
    let connection = SecretGenerator::id("conn").unwrap();
    let host = policy.host().unwrap();
    let tokens = Zeroizing::new(
        serde_json::to_vec(&json!({
            "access_token":provider.token(),"refresh_token":null,"token_type":"Bearer",
            "scope":"read_api read_repository","expires_in":3600
        }))
        .unwrap(),
    );
    let encrypted = serde_json::to_value(
        store
            .provider_secret_cipher()
            .unwrap()
            .seal(
                &ProviderSecretContext {
                    binding_id: connection.clone(),
                    provider: GitProvider::Gitlab.into(),
                    host: host.clone(),
                    purpose: "gitlab_tokens".into(),
                    revision: 1,
                },
                &tokens,
            )
            .unwrap(),
    )
    .unwrap();
    let pg_password = Zeroizing::new(
        std::fs::read_to_string(config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&config.host)
        .port(config.postgres_port)
        .user(&config.postgres_user)
        .password(pg_password.trim())
        .dbname(&config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{namespace}\""))
        .unwrap();
    sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab',$2,$3,'42','fixture','active',1,$4,now()+interval '1 hour')", &[&connection,&host,&ctx.actor_id(),&encrypted]).unwrap();
    sql.execute("UPDATE project_repositories SET host=$3,remote_id='101',remote_full_name='fixture/source',connection_id=$1,authorization_status='authorized',revision=2 WHERE id=$2", &[&connection,&repository.id,&host]).unwrap();

    let head = "a".repeat(40);
    let prepared = CodeReviewStore::new(&mut store)
        .prepare(
            &ctx,
            &project.id,
            &repository.id,
            7,
            "review-request-1",
            &head,
            &policy,
        )
        .unwrap();
    assert!(prepared.input.is_some());
    let id = prepared.review["id"].as_str().unwrap();
    assert!(!prepared.review.as_object().unwrap().contains_key("input"));
    let replay = CodeReviewStore::new(&mut store)
        .prepare(
            &ctx,
            &project.id,
            &repository.id,
            7,
            "review-request-1",
            &head,
            &policy,
        )
        .unwrap();
    assert!(replay.input.is_none());
    assert_eq!(replay.review["id"], id);
    assert!(
        CodeReviewStore::new(&mut store)
            .prepare(
                &ctx,
                &project.id,
                &repository.id,
                7,
                "review-request-1",
                &"c".repeat(40),
                &policy
            )
            .is_err()
    );
    let report = json!({"summary":"No demonstrated defect.","findings":[],"limitations":["Static diff only."]});
    sql.execute(
        "UPDATE project_repositories SET source_generation=source_generation+1 WHERE id=$1",
        &[&repository.id],
    )
    .unwrap();
    let result = CodeReviewStore::new(&mut store)
        .finish(
            &ctx,
            &project.id,
            &repository.id,
            7,
            id,
            Some(report.clone()),
            "fixture-model",
            &policy,
        )
        .unwrap();
    assert_eq!(result["status"], "completed");
    let replay = CodeReviewStore::new(&mut store)
        .prepare(
            &ctx,
            &project.id,
            &repository.id,
            7,
            "review-request-1",
            &head,
            &policy,
        )
        .unwrap();
    assert!(replay.input.is_none());
    assert_eq!(replay.review["status"], "completed");
    let prepared = CodeReviewStore::new(&mut store)
        .prepare(
            &ctx,
            &project.id,
            &repository.id,
            7,
            "review-request-stale",
            &head,
            &policy,
        )
        .unwrap();
    sql.execute(
        "UPDATE project_repositories SET source_generation=source_generation+1 WHERE id=$1",
        &[&repository.id],
    )
    .unwrap();
    provider.control(json!({"pr_head":"c".repeat(40)}));
    let result = CodeReviewStore::new(&mut store)
        .finish(
            &ctx,
            &project.id,
            &repository.id,
            7,
            prepared.review["id"].as_str().unwrap(),
            Some(report),
            "fixture-model",
            &policy,
        )
        .unwrap();
    assert_eq!(result["status"], "stale");
    assert!(result["report"]["summary"].is_string());
    // Re-consent is a different authority revision, even when the PR SHAs stay fixed.
    provider.control(json!({}));
    let consent_claim = CodeReviewStore::new(&mut store)
        .prepare(
            &ctx,
            &project.id,
            &repository.id,
            7,
            "review-request-consent",
            &head,
            &policy,
        )
        .unwrap();
    sql.execute("UPDATE project_repositories SET revision=revision+1,source_generation=source_generation+1 WHERE id=$1", &[&repository.id]).unwrap();
    let report =
        json!({"summary":"Pinned result","findings":[],"limitations":["Static diff only."]});
    assert!(
        CodeReviewStore::new(&mut store)
            .finish(
                &ctx,
                &project.id,
                &repository.id,
                7,
                consent_claim.review["id"].as_str().unwrap(),
                Some(report),
                "fixture-model",
                &policy
            )
            .is_err()
    );
    assert!(
        sql.query_one(
            "SELECT report IS NULL FROM pull_request_reviews WHERE id=$1",
            &[&consent_claim.review["id"].as_str().unwrap()]
        )
        .unwrap()
        .get::<_, bool>(0)
    );
    provider.control(json!({"partial_diff":true}));
    assert!(
        CodeReviewStore::new(&mut store)
            .prepare(
                &ctx,
                &project.id,
                &repository.id,
                7,
                "review-request-partial",
                &head,
                &policy
            )
            .is_err()
    );
    assert_eq!(
        sql.query_one("SELECT count(*) FROM pull_request_reviews", &[])
            .unwrap()
            .get::<_, i64>(0),
        3
    );
    sql.execute("UPDATE pull_request_reviews SET status='running',created_at=now()-interval '4 minutes' WHERE id=$1",&[&id]).unwrap();
    assert_eq!(
        CodeReviewStore::new(&mut store)
            .get(&ctx, &project.id, &repository.id, 7, id, &policy)
            .unwrap()["status"],
        "interrupted"
    );
    provider.control(json!({}));
    let revoked_claim = CodeReviewStore::new(&mut store)
        .prepare(
            &ctx,
            &project.id,
            &repository.id,
            7,
            "review-request-revoked",
            &head,
            &policy,
        )
        .unwrap();
    sql.execute(
        "UPDATE git_connections SET status='revoked',revision=revision+1 WHERE id=$1",
        &[&connection],
    )
    .unwrap();
    assert!(
        CodeReviewStore::new(&mut store)
            .get(&ctx, &project.id, &repository.id, 7, id, &policy)
            .is_err()
    );
    assert!(CodeReviewStore::new(&mut store).finish(&ctx,&project.id,&repository.id,7,revoked_claim.review["id"].as_str().unwrap(),Some(json!({"summary":"Pinned result","findings":[],"limitations":["Static diff only."]})),"fixture-model",&policy).is_err());
    assert!(
        sql.query_one(
            "SELECT report IS NULL FROM pull_request_reviews WHERE id=$1",
            &[&revoked_claim.review["id"].as_str().unwrap()]
        )
        .unwrap()
        .get::<_, bool>(0)
    );
    drop(store);
    sql.batch_execute(&format!(r#"DROP SCHEMA "{namespace}" CASCADE"#))
        .unwrap();
    std::fs::remove_file(
        config
            .secrets_directory
            .join(format!("{namespace}_master_key_v1")),
    )
    .unwrap();
}
