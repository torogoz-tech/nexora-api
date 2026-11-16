#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use nexora_platform::*;
use nexora_platform_store::{GitNetworkPolicy, PlatformStore, SecretGenerator, WebhookProof};
use serde_json::json;
use sha2::Sha256;
use work_item_fixture::WorkItemFixture;
use zeroize::Zeroizing;
#[test]
#[ignore = "requires PostgreSQL; signed webhooks -> durable automatic jobs without developer credentials"]
fn signed_webhooks_queue_coalesce_retry_and_recheck_authority_without_manual_sync() {
    let mut f = WorkItemFixture::new();
    f.task("Exercise shared fixture policy");
    let ctx = f.ctx.clone();
    let project = f.project.id.clone();
    let repo = f
        .store
        .register_repository(
            &ctx,
            &project,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "fixture/source".into(),
            },
        )
        .unwrap();
    let password = Zeroizing::new(
        std::fs::read_to_string(f.config.secrets_directory.join("postgres_password")).unwrap(),
    );
    let mut sql = postgres::Config::new()
        .host(&f.config.host)
        .port(f.config.postgres_port)
        .user(&f.config.postgres_user)
        .password(password.trim())
        .dbname(&f.config.postgres_database)
        .connect(postgres::NoTls)
        .unwrap();
    sql.batch_execute(&format!("SET search_path TO \"{}\"", f.namespace))
        .unwrap();
    let connection = SecretGenerator::id("gcn").unwrap();
    sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at) VALUES($1,'gitlab','gitlab.com',$2,'42','fixture','active',1,'{}',now()+interval '1 hour')",&[&connection,&ctx.actor_id()]).unwrap();
    sql.execute("UPDATE project_repositories SET remote_id='101',remote_full_name='fixture/source',connection_id=$1,authorization_status='authorized',revision=2 WHERE id=$2",&[&connection,&repo.id]).unwrap();
    let mut key = Zeroizing::new(vec![0u8; 32]);
    getrandom::fill(&mut key).unwrap();
    let policy = GitNetworkPolicy::default();
    let hook = f
        .store
        .register_gitlab_webhook(
            &ctx,
            &project,
            &repo.id,
            &GitlabWebhookRegistration {
                signing_token: format!("whsec_{}", STANDARD.encode(&*key)),
                expected_revision: 2,
            },
            &policy,
        )
        .unwrap();
    let delivery = |id: &str, sha: &str| {
        let body = serde_json::to_vec(
            &json!({"project_id":101,"object_kind":"push","ref":"refs/heads/main","after":sha}),
        )
        .unwrap();
        let timestamp = time::OffsetDateTime::now_utc().unix_timestamp().to_string();
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&key).unwrap();
        mac.update(format!("{id}.{timestamp}.").as_bytes());
        mac.update(&body);
        (
            WebhookProof {
                delivery_id: id.into(),
                event: "Push Hook".into(),
                signature: format!("v1,{}", STANDARD.encode(mac.finalize().into_bytes())),
                timestamp: Some(timestamp),
            },
            body,
        )
    };
    let (proof, body) = delivery("automatic-1", &"a".repeat(40));
    assert!(
        !f.store
            .receive_gitlab_webhook(&hook.id, &proof, &body, &policy)
            .unwrap()
            .duplicate
    );
    assert_eq!(
        sql.query_one(
            "SELECT count(*) FROM repository_analysis_requests WHERE state='pending'",
            &[]
        )
        .unwrap()
        .get::<_, i64>(0),
        1
    );
    assert!(
        f.store
            .receive_gitlab_webhook(&hook.id, &proof, &body, &policy)
            .unwrap()
            .duplicate
    );
    assert_eq!(
        sql.query_one("SELECT count(*) FROM repository_analysis_requests", &[])
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    let (mut invalid, invalid_body) = delivery("invalid-signature", &"d".repeat(40));
    invalid.signature = "v1,invalid".into();
    assert!(
        f.store
            .receive_gitlab_webhook(&hook.id, &invalid, &invalid_body, &policy)
            .is_err()
    );
    assert_eq!(
        sql.query_one("SELECT count(*) FROM repository_analysis_requests", &[])
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    // Automatic work uses current source consent, not the temporary developer login.
    sql.execute(
        "UPDATE sessions SET revoked_at=clock_timestamp() WHERE id=$1",
        &[&ctx.credential_id()],
    )
    .unwrap();
    let mut restarted = PlatformStore::connect(&f.config, &f.namespace).unwrap();
    let first = restarted
        .claim_repository_job("auto-worker", 60)
        .unwrap()
        .unwrap();
    assert_eq!(
        first.job.pinned_commit.as_deref(),
        Some("a".repeat(40).as_str())
    );
    let auto = sql
        .query_one(
            "SELECT credential_method,source_event_id FROM repository_jobs WHERE id=$1",
            &[&first.job.id],
        )
        .unwrap();
    assert_eq!(auto.get::<_, String>(0), "webhook");
    assert!(!auto.get::<_, String>(1).is_empty());
    let (p, b) = delivery("automatic-2", &"b".repeat(40));
    restarted
        .receive_gitlab_webhook(&hook.id, &p, &b, &policy)
        .unwrap();
    let (p, b) = delivery("automatic-3", &"c".repeat(40));
    restarted
        .receive_gitlab_webhook(&hook.id, &p, &b, &policy)
        .unwrap();
    assert!(
        restarted
            .pin_repository_job(&first, &"a".repeat(40))
            .is_err()
    );
    assert!(
        restarted
            .claim_repository_job("parallel-worker", 60)
            .unwrap()
            .is_none(),
        "one active job per repository"
    );
    restarted
        .fail_repository_job(&first, RepositoryJobFailure::SourceInvalid)
        .unwrap();
    let latest = restarted
        .claim_repository_job("auto-worker", 60)
        .unwrap()
        .unwrap();
    assert_eq!(
        latest.job.pinned_commit.as_deref(),
        Some("c".repeat(40).as_str())
    );
    let retry = restarted
        .fail_repository_job(&latest, RepositoryJobFailure::SourceUnavailable)
        .unwrap();
    assert_eq!(retry.state, RepositoryJobState::Queued);
    assert!(
        restarted
            .claim_repository_job("auto-worker", 60)
            .unwrap()
            .is_none(),
        "transient retries wait"
    );
    sql.execute(
        "UPDATE repository_jobs SET available_at=clock_timestamp() WHERE id=$1",
        &[&retry.id],
    )
    .unwrap();
    let retry = restarted
        .claim_repository_job("auto-worker", 60)
        .unwrap()
        .unwrap();
    assert_eq!(retry.job.id, latest.job.id);
    assert_eq!(retry.job.attempt, 2);
    let queued = restarted
        .fail_repository_job(&retry, RepositoryJobFailure::StorageUnavailable)
        .unwrap();
    assert_eq!(queued.state, RepositoryJobState::Queued);
    sql.execute(
        "UPDATE repository_jobs SET available_at=clock_timestamp() WHERE id=$1",
        &[&queued.id],
    )
    .unwrap();
    let last = restarted
        .claim_repository_job("auto-worker", 60)
        .unwrap()
        .unwrap();
    assert_eq!(last.job.attempt, 3);
    assert_eq!(
        restarted
            .fail_repository_job(&last, RepositoryJobFailure::SourceUnavailable)
            .unwrap()
            .state,
        RepositoryJobState::Failed,
        "retry budget is finite"
    );
    let (p, b) = delivery("automatic-4", &"d".repeat(40));
    restarted
        .receive_gitlab_webhook(&hook.id, &p, &b, &policy)
        .unwrap();
    let retry = restarted
        .claim_repository_job("auto-worker", 60)
        .unwrap()
        .unwrap();
    let generation: i64 = sql
        .query_one(
            "SELECT source_generation FROM project_repositories WHERE id=$1",
            &[&repo.id],
        )
        .unwrap()
        .get(0);
    let (p, b) = delivery("deleted-branch", &"0".repeat(40));
    restarted
        .receive_gitlab_webhook(&hook.id, &p, &b, &policy)
        .unwrap();
    assert_eq!(
        sql.query_one(
            "SELECT source_generation FROM project_repositories WHERE id=$1",
            &[&repo.id]
        )
        .unwrap()
        .get::<_, i64>(0),
        generation
    );
    sql.execute(
        "UPDATE gitlab_webhooks SET active=false,revision=revision+1 WHERE id=$1",
        &[&hook.id],
    )
    .unwrap();
    assert!(restarted.renew_repository_job(&retry, 60).is_err());
    assert_eq!(
        sql.query_one(
            "SELECT state FROM repository_jobs WHERE id=$1",
            &[&retry.job.id]
        )
        .unwrap()
        .get::<_, String>(0),
        "failed"
    );
    assert!(
        restarted
            .claim_repository_job("auto-worker", 60)
            .unwrap()
            .is_none()
    );
}
