#[path = "support/git_author_fixture.rs"]
mod git_author_fixture;
use git_author_fixture::GitAuthorFixture;
use nexora_platform::*;
use serde_json::json;

#[test]
#[ignore = "requires PostgreSQL; project commit identity, archived attribution and administrator privacy"]
fn project_commits_resolve_public_member_identity_without_exposing_administrator_profiles() {
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
    f.send("project-author", json!({"project_id":101,"object_kind":"push","ref":"refs/heads/main","after":sha,"commits":[{"id":sha,"message":"Attributed change","author":{"name":"Git name","email":"dev@example.com"}}]}));
    f.sql
        .execute(
            "UPDATE actors SET name='Public name' WHERE id=$1",
            &[&developer.actor_id()],
        )
        .unwrap();
    f.sql.execute("INSERT INTO member_positions(actor_id,position) VALUES($1,'Technical lead') ON CONFLICT(actor_id) DO UPDATE SET position=EXCLUDED.position", &[&developer.actor_id()]).unwrap();
    let project = f.work.project.id.clone();
    let profile = f
        .work
        .store
        .project_profile(&manager, &project, None)
        .unwrap();
    let author = &profile["activity"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == "commit")
        .unwrap()["author"];
    assert_eq!(author["id"], developer.actor_id());
    assert_eq!(author["name"], "Public name");
    assert_eq!(author["position"], "Technical lead");
    assert!(author.get("email").is_none());
    f.sql
        .execute(
            "UPDATE actors SET active=false WHERE id=$1",
            &[&developer.actor_id()],
        )
        .unwrap();
    let profile = f
        .work
        .store
        .project_profile(&manager, &project, None)
        .unwrap();
    assert!(
        profile["activity"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["author"]["name"] == "Public name")
    );
    f.sql
        .execute(
            "UPDATE actor_roles SET role='super_admin',all_projects=true WHERE actor_id=$1",
            &[&developer.actor_id()],
        )
        .unwrap();
    let profile = f
        .work
        .store
        .project_profile(&manager, &project, None)
        .unwrap();
    let author = &profile["activity"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["kind"] == "commit")
        .unwrap()["author"];
    assert!(author["id"].is_null());
    assert_eq!(author["name"], "Git name");
    assert_eq!(author["position"], "");
}
