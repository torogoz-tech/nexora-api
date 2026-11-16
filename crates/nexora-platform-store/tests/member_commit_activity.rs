#[path = "support/git_author_fixture.rs"]
mod git_author_fixture;
use git_author_fixture::GitAuthorFixture;
use nexora_platform::*;
use serde_json::json;
#[test]
#[ignore = "requires PostgreSQL; latest hundred messages, branches and chronological order"]
fn member_activity_has_latest_hundred_commit_messages_and_branches() {
    let mut f = GitAuthorFixture::new();
    let developer = f.member("credited", MemberRole::Developer);
    let manager = f.member("manager", MemberRole::Manager);
    f.work
        .store
        .create_git_author_alias(
            &manager,
            developer.actor_id(),
            &GitAuthorAliasCreate {
                provider: "gitlab".into(),
                host: "gitlab.com".into(),
                email: "dev@example.com".into(),
                project_id: Some(f.work.project.id.clone()),
                reason: "Associate author".into(),
            },
        )
        .unwrap();
    let sha = "a".repeat(40);
    f.send("activity-seed",json!({"project_id":101,"object_kind":"push","ref":"refs/heads/main","after":sha,"commits":[{"id":sha,"message":"Initial commit","author":{"name":"Developer","email":"dev@example.com"}}]}));
    f.sql
        .execute(
            "UPDATE git_commit_authorship SET occurred_at='2025-01-01'",
            &[],
        )
        .unwrap();
    f.sql.execute("INSERT INTO git_commit_authorship(id,provider,host,remote_repository_id,repository_id,project_id,commit_sha,author_name,author_email,actor_id,alias_id,scope_id,delivery_id,occurred_at,commit_message) SELECT 'gco-test-'||s,g.provider,g.host,g.remote_repository_id,g.repository_id,g.project_id,lpad(s::text,40,'0'),g.author_name,g.author_email,g.actor_id,g.alias_id,g.scope_id,g.delivery_id,'2026-01-01'::timestamptz+s*interval '1 hour','Change '||s FROM git_commit_authorship g CROSS JOIN generate_series(1,105) s",&[]).unwrap();
    f.sql.execute("INSERT INTO git_commit_branches(repository_id,commit_sha,branch_name) SELECT repository_id,commit_sha,'feature/authentication' FROM git_commit_authorship WHERE id='gco-test-105'",&[]).unwrap();
    let profile = f
        .work
        .store
        .member_profile(&manager, developer.actor_id(), Some(2026))
        .unwrap();
    let events = profile["activity"].as_array().unwrap();
    assert_eq!(events.len(), 100);
    assert_eq!(events[0]["title"], "Change 6");
    assert_eq!(events[99]["title"], "Change 105");
    assert!(events.iter().all(|event| {
        event["repository_name"]
            .as_str()
            .is_some_and(|name| !name.is_empty())
    }));
    assert_eq!(events[99]["branches"], json!(["feature/authentication"]));
    assert_eq!(events[99]["commit_sha"], format!("{:0>40}", 105));
    assert!(
        events
            .windows(2)
            .all(|pair| pair[0]["created_at"].as_str().unwrap()
                <= pair[1]["created_at"].as_str().unwrap())
    );
    assert!(
        f.work
            .store
            .member_profile(&developer, developer.actor_id(), Some(2026))
            .is_ok()
    );
}
