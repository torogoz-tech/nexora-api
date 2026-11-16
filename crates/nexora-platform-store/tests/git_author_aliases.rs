#[path = "support/git_author_fixture.rs"]
mod git_author_fixture;
use git_author_fixture::GitAuthorFixture;
use nexora_platform::*;
use serde_json::{Value, json};
fn alias(f: &GitAuthorFixture, email: &str) -> GitAuthorAliasCreate {
    GitAuthorAliasCreate {
        provider: "gitlab".into(),
        host: "gitlab.com".into(),
        email: email.into(),
        project_id: Some(f.work.project.id.clone()),
        reason: "Associate observed author".into(),
    }
}
fn payload(sha: &str, email: &str) -> Value {
    json!({"project_id":101,"object_kind":"push","ref":"refs/heads/main","after":sha,"commits":[{"id":sha,"author":{"name":"Different Git author","email":email}}]})
}
#[test]
#[ignore = "requires PostgreSQL; signed author attribution, ratings, exact scope and deduplication"]
fn signed_author_commits_are_independent_of_sender_and_count_once() {
    let mut f = GitAuthorFixture::new();
    let dev = f.member("credited", MemberRole::Developer);
    let manager = f.member("manager", MemberRole::Manager);
    let input = alias(&f, " Real.Author@Example.com ");
    assert!(
        f.work
            .store
            .create_git_author_alias(&dev, dev.actor_id(), &input)
            .is_err()
    );
    let mapping = f
        .work
        .store
        .create_git_author_alias(&manager, dev.actor_id(), &input)
        .unwrap();
    assert_eq!(mapping["email"], "real.author@example.com");
    assert!(
        mapping["project_id"].is_null(),
        "Git aliases must always cover the organization"
    );
    assert!(
        f.work
            .store
            .create_git_author_alias(
                &f.work.ctx,
                dev.actor_id(),
                &GitAuthorAliasCreate {
                    project_id: None,
                    ..input.clone()
                }
            )
            .is_err()
    );
    let sha = "a".repeat(40);
    let body = payload(&sha, "real.author@example.com");
    assert!(!f.send("author-first", body.clone()).duplicate);
    assert!(f.send("author-first", body.clone()).duplicate);
    assert!(!f.send("author-again", body).duplicate);
    let rows = f
        .work
        .store
        .git_authors(&manager, &GitAuthorQuery::default())
        .unwrap();
    assert_eq!(rows["items"].as_array().unwrap().len(), 1);
    assert_eq!(rows["items"][0]["actor_id"], dev.actor_id());
    assert_eq!(
        f.sql
            .query_one("SELECT count(*) FROM developer_push_activity", &[])
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    f.sql.batch_execute("UPDATE labor_calendar_settings SET starts_on=(clock_timestamp() AT TIME ZONE timezone)::date,workdays=ARRAY[1,2,3,4,5,6,7]").unwrap();
    f.work.store.refresh_developer_ratings().unwrap();
    let year = f
        .sql
        .query_one("SELECT extract(year FROM clock_timestamp())::integer", &[])
        .unwrap()
        .get(0);
    let report = f
        .work
        .store
        .member_ratings(&manager, dev.actor_id(), year)
        .unwrap();
    let today = report["daily"].as_array().unwrap().last().unwrap();
    assert_eq!(today["push"], false);
    assert_eq!(today["authored"], true);
    assert_eq!(today["score"], 5.0);
    assert_eq!(today["entries"][0]["reason"], "author_commit_recorded");
    let profile = f
        .work
        .store
        .member_profile(&f.work.ctx, dev.actor_id(), Some(year))
        .unwrap();
    assert!(
        profile["activity"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "commit")
    );
    assert_eq!(
        profile["contributions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["count"].as_i64().unwrap())
            .sum::<i64>(),
        1
    );
    let revoked = f
        .work
        .store
        .revoke_git_author_alias(
            &manager,
            dev.actor_id(),
            mapping["id"].as_str().unwrap(),
            &GitAuthorAliasRevoke {
                expected_revision: 1,
                reason: "Stop future matching".into(),
            },
        )
        .unwrap();
    assert!(!revoked["revoked_at"].is_null());
    f.send(
        "after-revoke",
        payload(&"b".repeat(40), "real.author@example.com"),
    );
    let rows = f
        .work
        .store
        .git_authors(&manager, &GitAuthorQuery::default())
        .unwrap();
    assert!(
        rows["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["commit_sha"] == sha && v["actor_id"] == dev.actor_id())
    );
    assert!(
        rows["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["commit_sha"] == "b".repeat(40) && v["actor_id"].is_null())
    );
}
#[test]
#[ignore = "requires PostgreSQL; late alias mapping, roles, wrongscope and redacted metadata"]
fn late_alias_maps_only_unmatched_commits_without_name_guessing() {
    let mut f = GitAuthorFixture::new();
    let dev = f.member("late", MemberRole::Developer);
    let lead = f.member("lead", MemberRole::TechnicalLead);
    let pmo = f.member("pmo", MemberRole::Pmo);
    f.send(
        "unknown-first",
        payload(&"c".repeat(40), "late@example.com"),
    );
    f.send("redacted", payload(&"d".repeat(40), "[REDACTED]"));
    f.send(
        "overlong",
        payload(
            &"e".repeat(40),
            &format!("{}{}", "x".repeat(321), "@example.com"),
        ),
    );
    let m = f
        .work
        .store
        .create_git_author_alias(&lead, dev.actor_id(), &alias(&f, "late@example.com"))
        .unwrap();
    let rows = f
        .work
        .store
        .git_authors(&lead, &GitAuthorQuery::default())
        .unwrap();
    assert_eq!(
        rows["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["actor_id"] == dev.actor_id())
            .count(),
        1
    );
    let mut global = alias(&f, "other@example.com");
    global.project_id = None;
    assert!(
        f.work
            .store
            .create_git_author_alias(&lead, dev.actor_id(), &global)
            .is_ok()
    );
    let foreign = f
        .work
        .store
        .create_project(&f.work.ctx, "Foreign alias", None)
        .unwrap();
    global.project_id = Some(foreign.id);
    assert!(
        f.work
            .store
            .create_git_author_alias(&lead, dev.actor_id(), &global)
            .is_err()
    );
    assert!(
        f.work
            .store
            .create_git_author_alias(&pmo, dev.actor_id(), &alias(&f, "second@example.com"))
            .is_ok()
    );
    assert!(
        f.work
            .store
            .create_git_author_alias(
                &f.work.ctx,
                f.work.ctx.actor_id(),
                &alias(&f, "administrator@example.com")
            )
            .is_ok()
    );
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            dev.actor_id(),
            "Scoped reader",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead],
                project_ids: vec![f.work.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let api = f.work.store.authenticate(&key.key).unwrap();
    assert!(
        f.work
            .store
            .create_git_author_alias(&api, dev.actor_id(), &alias(&f, "forged@example.com"))
            .is_err()
    );
    assert_eq!(
        f.work.store.git_author_options(&api, None).unwrap()["can_manage"],
        false
    );
    f.work
        .store
        .revoke_git_author_alias(
            &lead,
            dev.actor_id(),
            m["id"].as_str().unwrap(),
            &GitAuthorAliasRevoke {
                expected_revision: 1,
                reason: "Replace mapping".into(),
            },
        )
        .unwrap();
    f.work
        .store
        .create_git_author_alias(&lead, lead.actor_id(), &alias(&f, "late@example.com"))
        .unwrap();
    let rows = f
        .work
        .store
        .git_authors(&lead, &GitAuthorQuery::default())
        .unwrap();
    assert_eq!(
        rows["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["commit_sha"] == "c".repeat(40))
            .unwrap()["actor_id"],
        dev.actor_id()
    );
    f.send(
        "replacement-future",
        payload(&"8".repeat(40), "late@example.com"),
    );
    let rows = f
        .work
        .store
        .git_authors(&lead, &GitAuthorQuery::default())
        .unwrap();
    assert_eq!(
        rows["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["commit_sha"] == "8".repeat(40))
            .unwrap()["actor_id"],
        lead.actor_id()
    );
}

#[test]
#[ignore = "requires PostgreSQL; key attenuation, inactive cleanup and nonrated author roles"]
fn aliases_preserve_key_scope_and_allow_inactive_cleanup_without_rating_managers() {
    let mut f = GitAuthorFixture::new();
    let dev = f.member("scoped", MemberRole::Developer);
    let manager = f.member("manager", MemberRole::Manager);
    let foreign = f
        .work
        .store
        .create_project(&f.work.ctx, "Other project", None)
        .unwrap();
    f.work
        .store
        .change_member_role(
            &f.work.ctx,
            dev.actor_id(),
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![f.work.project.id.clone(), foreign.id.clone()],
                expected_revision: 1,
            },
        )
        .unwrap();
    let own = f
        .work
        .store
        .create_git_author_alias(&manager, dev.actor_id(), &alias(&f, "owned@example.com"))
        .unwrap();
    f.work
        .store
        .create_git_author_alias(
            &f.work.ctx,
            dev.actor_id(),
            &GitAuthorAliasCreate {
                email: "foreign@example.com".into(),
                project_id: Some(foreign.id),
                ..alias(&f, "ignored@example.com")
            },
        )
        .unwrap();
    f.work
        .store
        .create_git_author_alias(
            &f.work.ctx,
            dev.actor_id(),
            &GitAuthorAliasCreate {
                email: "global@example.com".into(),
                project_id: None,
                ..alias(&f, "ignored@example.com")
            },
        )
        .unwrap();
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            dev.actor_id(),
            "Project-only aliases",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead],
                project_ids: vec![f.work.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let api = f.work.store.authenticate(&key.key).unwrap();
    let visible = f
        .work
        .store
        .git_author_aliases(
            &api,
            &GitAuthorQuery {
                member_id: Some(dev.actor_id().into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(visible["items"].as_array().unwrap().len(), 3);
    assert!(
        visible["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|alias| alias["project_id"].is_null())
    );
    assert!(
        visible["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|alias| alias["id"] == own["id"])
    );
    f.sql
        .execute(
            "UPDATE actors SET active=false WHERE id=$1",
            &[&dev.actor_id()],
        )
        .unwrap();
    assert!(
        f.work
            .store
            .revoke_git_author_alias(
                &manager,
                dev.actor_id(),
                own["id"].as_str().unwrap(),
                &GitAuthorAliasRevoke {
                    expected_revision: 1,
                    reason: "Inactive member cleanup".into()
                }
            )
            .is_ok()
    );
    f.work
        .store
        .create_git_author_alias(
            &manager,
            manager.actor_id(),
            &alias(&f, "manager@example.com"),
        )
        .unwrap();
    f.send(
        "manager-author",
        payload(&"f".repeat(40), "manager@example.com"),
    );
    f.work.store.refresh_developer_ratings().unwrap();
    assert_eq!(
        f.sql
            .query_one(
                "SELECT count(*) FROM developer_rating_entries WHERE actor_id=$1",
                &[&manager.actor_id()]
            )
            .unwrap()
            .get::<_, i64>(0),
        0
    );
}
#[test]
#[ignore = "requires PostgreSQL; physical repository identity across project inscriptions"]
fn physical_commit_reobservation_reuses_first_receipt_and_profile_dedup() {
    let mut f = GitAuthorFixture::new();
    let dev = f.member("physical", MemberRole::Developer);
    f.work
        .store
        .create_git_author_alias(
            &f.work.ctx,
            dev.actor_id(),
            &GitAuthorAliasCreate {
                project_id: None,
                ..alias(&f, "same@local")
            },
        )
        .unwrap();
    let sha = "abcdef0123".repeat(4);
    f.send("physical-one", payload(&sha, "same@local"));
    f.sql
        .execute(
            "UPDATE git_commit_authorship SET occurred_at=clock_timestamp()-interval '1 day'",
            &[],
        )
        .unwrap();
    let mut again = payload(&sha, "same@local");
    again["commits"][0]["id"] = json!(sha.to_uppercase());
    f.send("physical-two", again);
    assert_eq!(
        f.sql
            .query_one("SELECT count(*) FROM git_commit_authorship", &[])
            .unwrap()
            .get::<_, i64>(0),
        1
    );
    assert!(f.sql.query_one("SELECT max(occurred_at)<clock_timestamp()-interval '23 hours' FROM git_commit_authorship",&[]).unwrap().get::<_,bool>(0));
    let year = f
        .sql
        .query_one("SELECT extract(year FROM clock_timestamp())::integer", &[])
        .unwrap()
        .get(0);
    let profile = f
        .work
        .store
        .member_profile(&f.work.ctx, dev.actor_id(), Some(year))
        .unwrap();
    assert_eq!(
        profile["contributions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["count"].as_i64().unwrap())
            .sum::<i64>(),
        1
    );
}
#[test]
#[ignore = "requires PostgreSQL; alias binding racing verified webhook"]
fn concurrent_alias_and_webhook_have_one_attribution_without_deadlock() {
    use nexora_platform_store::{GitNetworkPolicy, PlatformStore};
    use std::sync::{Arc, Barrier};
    let mut f = GitAuthorFixture::new();
    let dev = f.member("concurrent", MemberRole::Developer);
    let manager = f.member("coordinator", MemberRole::Manager);
    let input = alias(&f, "race@example.com");
    let (proof, body) = f.proof(
        "racing-delivery",
        payload(&"9".repeat(40), "race@example.com"),
    );
    let barrier = Arc::new(Barrier::new(2));
    let config = f.work.config.clone();
    let namespace = f.work.namespace.clone();
    let first_barrier = barrier.clone();
    let actor = dev.actor_id().to_string();
    let thread = std::thread::spawn(move || {
        let mut store = PlatformStore::connect(&config, &namespace).unwrap();
        first_barrier.wait();
        store
            .create_git_author_alias(&manager, &actor, &input)
            .unwrap();
    });
    let mut store = PlatformStore::connect(&f.work.config, &f.work.namespace).unwrap();
    barrier.wait();
    store
        .receive_gitlab_webhook(&f.hook, &proof, &body, &GitNetworkPolicy::default())
        .unwrap();
    thread.join().unwrap();
    assert_eq!(
        f.sql
            .query_one(
                "SELECT count(*) FROM git_commit_authorship WHERE actor_id=$1",
                &[&dev.actor_id()]
            )
            .unwrap()
            .get::<_, i64>(0),
        1
    );
}

#[test]
#[ignore = "requires PostgreSQL; GitHub signed webhook keeps sender distinct from aliased author"]
fn github_signed_push_attributes_author_without_replacing_authenticated_sender() {
    use hmac::{Hmac, Mac};
    use nexora_platform_store::{
        GitNetworkPolicy, ProviderSecretContext, SecretGenerator, WebhookProof,
    };
    use sha2::Sha256;
    let mut f = GitAuthorFixture::new();
    let author = f.member("github_author", MemberRole::Developer);
    let sender = f.member("github_sender", MemberRole::Developer);
    let policy = GitNetworkPolicy::default();
    let secret = zeroize::Zeroizing::new(SecretGenerator::token("github_hook").unwrap());
    let plain=zeroize::Zeroizing::new(serde_json::to_vec(&json!({"client_secret":SecretGenerator::token("client").unwrap(),"private_key_pem":SecretGenerator::token("unused_fixture_private_key").unwrap(),"webhook_secret":secret.as_str()})).unwrap());
    let sealed = serde_json::to_value(
        f.work
            .store
            .provider_secret_cipher()
            .unwrap()
            .seal(
                &ProviderSecretContext {
                    binding_id: "github-settings".into(),
                    provider: GitProvider::Github.into(),
                    host: "github.com".into(),
                    purpose: "github_app_secrets".into(),
                    revision: 1,
                },
                &plain,
            )
            .unwrap(),
    )
    .unwrap();
    f.sql.execute("INSERT INTO github_settings(singleton,app_id,client_id,slug,origin,api_origin,host,redirect_uri,enabled,revision,secrets,credential_revision) VALUES(true,'88','fixture-client','fixture','https://github.com','https://api.github.com','github.com',$1,true,1,$2,1)",&[&format!("{}/api/v1/git/github/callback",policy.callback_origin),&sealed]).unwrap();
    let repo = f
        .work
        .store
        .register_repository(
            &f.work.ctx,
            &f.work.project.id,
            &RepositoryRegistration {
                provider: GitProvider::Github,
                full_name: "fixture/github-authors".into(),
            },
        )
        .unwrap();
    let connection = SecretGenerator::id("github_connection").unwrap();
    f.sql.execute("INSERT INTO git_connections(id,provider,host,owner_actor_id,remote_account_id,remote_login,status,config_revision,tokens,expires_at,installation_id) VALUES($1,'github','github.com',$2,'42','sender','active',1,'{}',now()+interval '1 hour','7')",&[&connection,&sender.actor_id()]).unwrap();
    f.sql.execute("UPDATE project_repositories SET remote_id='101',remote_full_name='fixture/github-authors',connection_id=$1,authorization_status='authorized',revision=2 WHERE id=$2",&[&connection,&repo.id]).unwrap();
    f.work
        .store
        .create_git_author_alias(
            &f.work.ctx,
            author.actor_id(),
            &GitAuthorAliasCreate {
                provider: "github".into(),
                host: "github.com".into(),
                email: "actual.author@example.com".into(),
                project_id: Some(f.work.project.id.clone()),
                reason: "GitHub fixture author attribution".into(),
            },
        )
        .unwrap();
    let body=serde_json::to_vec(&json!({"installation":{"id":7},"repository":{"id":101},"sender":{"id":42},"ref":"refs/heads/main","before":"0".repeat(40),"after":"7".repeat(40),"commits":[{"id":"7".repeat(40),"author":{"name":"GitHub declared author","email":"actual.author@example.com","username":"not_the_sender"}}]})).unwrap();
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(&body);
    let signature = format!(
        "sha256={}",
        mac.finalize()
            .into_bytes()
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>()
    );
    let proof = WebhookProof {
        delivery_id: "github-author-proof".into(),
        event: "push".into(),
        signature,
        timestamp: None,
    };
    assert!(
        !f.work
            .store
            .receive_github_webhook(&proof, &body, &policy)
            .unwrap()
            .duplicate
    );
    assert!(
        f.work
            .store
            .receive_github_webhook(&proof, &body, &policy)
            .unwrap()
            .duplicate
    );
    assert_eq!(
        f.sql
            .query_one(
                "SELECT actor_id FROM developer_push_activity WHERE provider='github'",
                &[]
            )
            .unwrap()
            .get::<_, String>(0),
        sender.actor_id()
    );
    assert_eq!(
        f.sql
            .query_one(
                "SELECT actor_id FROM git_commit_authorship WHERE provider='github'",
                &[]
            )
            .unwrap()
            .get::<_, String>(0),
        author.actor_id()
    );
    assert_eq!(
        f.sql
            .query_one(
                "SELECT count(*) FROM git_commit_authorship WHERE provider='github'",
                &[]
            )
            .unwrap()
            .get::<_, i64>(0),
        1
    );
}
