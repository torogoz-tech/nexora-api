#[path = "support/git_author_fixture.rs"]
mod git_author_fixture;
use git_author_fixture::GitAuthorFixture;
use nexora_platform::*;
#[test]
#[ignore = "requires PostgreSQL"]
fn developers_and_leads_read_peer_and_project_git_heatmaps() {
    let mut f = GitAuthorFixture::new();
    f.sql.execute("INSERT INTO git_connection_projects(connection_id,project_id,granted_by) SELECT connection_id,project_id,$2 FROM project_repositories WHERE id=$1", &[&f.repository,&f.work.ctx.actor_id()]).unwrap();
    let peer = f.member("peer", MemberRole::Developer);
    f.sql.execute("INSERT INTO git_author_aliases(id,actor_id,provider,host,email,created_by) VALUES('alias-heatmap',$1,'gitlab','gitlab.com','peer@example.test',$1)",&[&peer.actor_id()]).unwrap();
    f.sql.execute("INSERT INTO git_commit_authorship(id,provider,host,remote_repository_id,repository_id,project_id,commit_sha,author_name,author_email,actor_id,alias_id,scope_id,delivery_id) VALUES('commit-heatmap','gitlab','gitlab.com','101',$1,$2,$3,'Peer','peer@example.test',$4,'alias-heatmap',$5,'heatmap')",&[&f.repository,&f.work.project.id,&"1".repeat(40),&peer.actor_id(),&f.hook]).unwrap();
    for (name, role) in [
        ("viewer", MemberRole::Developer),
        ("lead", MemberRole::TechnicalLead),
    ] {
        let viewer = f.member(name, role);
        let profile = f
            .work
            .store
            .member_profile(&viewer, peer.actor_id(), Some(2026))
            .unwrap();
        assert!(
            !profile["contributions"].as_array().unwrap().is_empty(),
            "{profile}"
        );
        let project = f
            .work
            .store
            .project_profile(&viewer, &f.work.project.id, Some(2026))
            .unwrap();
        assert!(
            !project["contributions"].as_array().unwrap().is_empty(),
            "{project}"
        );
    }
}
