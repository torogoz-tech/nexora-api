#[path = "support/git_author_fixture.rs"]
mod git_author_fixture;
use git_author_fixture::GitAuthorFixture;
use nexora_platform::*;
use serde_json::json;

#[test]
#[ignore = "requires PostgreSQL; project activity includes authorized repository commits"]
fn project_activity_includes_commits_and_hides_installation_administrator() {
    let mut f = GitAuthorFixture::new();
    let manager = f.member("project-manager", MemberRole::Manager);
    let project = f.work.project.id.clone();
    let owner = f.work.ctx.actor_id().to_string();
    f.sql.execute("INSERT INTO git_connection_projects(connection_id,project_id,granted_by) SELECT connection_id,project_id,$2 FROM project_repositories WHERE id=$1", &[&f.repository,&owner]).unwrap();
    let sha = "b".repeat(40);
    f.send("project-profile-commit", json!({"project_id":101,"object_kind":"push","ref":"refs/heads/develop","after":sha,"commits":[{"id":sha,"message":"Deliver project activity","author":{"name":"Developer","email":"dev@example.com"}}]}));
    f.sql
        .execute(
            "INSERT INTO member_positions(actor_id,position) VALUES($1,'Project manager')",
            &[&manager.actor_id()],
        )
        .unwrap();
    // Activity must include every accessible repository, even beyond the six previews.
    f.sql.execute("INSERT INTO project_repositories(id,project_id,source_kind,local_name,authorization_status,updated_at) SELECT 'repo_preview_'||n,$1,'local_snapshot','Preview '||n,'authorized',now()+interval '1 minute' FROM generate_series(1,6) n", &[&project]).unwrap();
    let profile = f
        .work
        .store
        .project_profile(&manager, &project, None)
        .unwrap();
    assert_eq!(profile["repositories"].as_array().unwrap().len(), 6);
    assert!(
        profile["repositories"]
            .as_array()
            .unwrap()
            .iter()
            .all(|repository| repository["id"] != f.repository)
    );
    let commits: Vec<_> = profile["activity"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["kind"] == "commit")
        .collect();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["title"], "Deliver project activity");
    assert_eq!(commits[0]["repository_id"], f.repository);
    assert_eq!(commits[0]["commit_sha"], sha);
    assert_eq!(commits[0]["branches"], json!(["develop"]));
    assert_eq!(
        profile["contributions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|day| day["count"].as_i64().unwrap())
            .sum::<i64>(),
        1
    );
    assert!(
        profile["members"]
            .as_array()
            .unwrap()
            .iter()
            .all(|member| member["id"] != owner)
    );
    assert!(
        profile["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|member| member["id"] == manager.actor_id()
                && member["position"] == "Project manager")
    );
    let directory = f.work.store.organization_members(&manager).unwrap();
    assert!(
        directory
            .as_array()
            .unwrap()
            .iter()
            .all(|member| member["id"] != owner)
    );
    assert!(matches!(
        f.work.store.member_profile(&manager, &owner, None),
        Err(PlatformError::NotFound)
    ));
    assert!(
        f.work
            .store
            .member_profile(&f.work.ctx, &owner, None)
            .is_ok()
    );
    f.sql.execute("UPDATE git_connections SET status='revoked' WHERE id=(SELECT connection_id FROM project_repositories WHERE id=$1)", &[&f.repository]).unwrap();
    let revoked = f
        .work
        .store
        .project_profile(&manager, &project, None)
        .unwrap();
    assert!(revoked["activity"].as_array().unwrap().is_empty());
    assert!(revoked["contributions"].as_array().unwrap().is_empty());
}
