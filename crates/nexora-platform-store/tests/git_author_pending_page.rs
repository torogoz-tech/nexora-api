#[path = "support/git_author_fixture.rs"]
mod git_author_fixture;
use git_author_fixture::GitAuthorFixture;
use nexora_platform::*;
use serde_json::json;
#[test]
#[ignore = "requires PostgreSQL; excludes attributed authors before pagination"]
fn pending_authors_are_filtered_before_pagination() {
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
                email: "linked@example.com".into(),
                project_id: Some(f.work.project.id.clone()),
                reason: String::new(),
            },
        )
        .unwrap();
    for (index, email) in [
        "linked@example.com",
        "pending@example.com",
        "linked@example.com",
        "other@example.com",
    ]
    .iter()
    .enumerate()
    {
        let sha = format!("{:040x}", index + 1);
        f.send(&format!("page-{index}"), json!({"project_id":101,"object_kind":"push","ref":"refs/heads/main","after":sha,"commits":[{"id":sha,"message":"Improve authentication flow","author":{"name":"Git author","email":email}}]}));
    }
    let mut query: GitAuthorQuery =
        serde_json::from_value(json!({"unlinked_only":true,"limit":1})).unwrap();
    let mut emails = Vec::new();
    loop {
        let page = f.work.store.git_authors(&manager, &query).unwrap();
        let items = page["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0]["actor_id"].is_null());
        emails.push(items[0]["author_email"].as_str().unwrap().to_owned());
        if page["next_after"].is_null() {
            break;
        }
        query.after = Some(page["next_after"].as_str().unwrap().to_owned());
        assert!(emails.len() < 3);
    }
    emails.sort();
    assert_eq!(emails, vec!["other@example.com", "pending@example.com"]);
    let message: String = f
        .sql
        .query_one(
            "SELECT commit_message FROM git_commit_authorship LIMIT 1",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(message, "Improve authentication flow");
    let all = f
        .work
        .store
        .git_authors(&manager, &GitAuthorQuery::default())
        .unwrap();
    assert_eq!(all["items"].as_array().unwrap().len(), 4);
    f.send("missing-email",json!({"project_id":101,"object_kind":"push","ref":"refs/heads/main","after":format!("{:040x}",99),"commits":[{"id":format!("{:040x}",99),"author":{"name":"invalid.email","email":"broken"}}]}));
    let pending: GitAuthorQuery = serde_json::from_value(json!({"unlinked_only":true})).unwrap();
    let before = f.work.store.git_authors(&manager, &pending).unwrap();
    assert_eq!(before["items"].as_array().unwrap().len(), 2);
    let id = before["items"][0]["id"].as_str().unwrap().to_string();
    assert!(
        f.work
            .store
            .archive_git_author(&developer, &id, true)
            .is_err()
    );
    f.work
        .store
        .archive_git_author(&manager, &id, true)
        .unwrap();
    assert_eq!(
        f.work.store.git_authors(&manager, &pending).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let archived: GitAuthorQuery =
        serde_json::from_value(json!({"unlinked_only":true,"archived_only":true})).unwrap();
    assert_eq!(
        f.work.store.git_authors(&manager, &archived).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    f.work
        .store
        .archive_git_author(&manager, &id, false)
        .unwrap();
    assert_eq!(
        f.work.store.git_authors(&manager, &pending).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
