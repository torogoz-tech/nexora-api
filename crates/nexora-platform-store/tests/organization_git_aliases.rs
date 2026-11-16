#[path = "support/git_author_fixture.rs"]
mod git_author_fixture;
use git_author_fixture::GitAuthorFixture;
use nexora_platform::*;

#[test]
#[ignore = "requires PostgreSQL; organization-wide aliases and recovery of imported work"]
fn aliases_migrate_and_attribute_past_and_future_work_across_projects() {
    let mut f = GitAuthorFixture::new();
    let dev = f.member("global-author", MemberRole::Developer);
    let lead = f.member("global-lead", MemberRole::TechnicalLead);
    let other = f
        .work
        .store
        .create_project(&f.work.ctx, "Other project", None)
        .unwrap();
    let repository = f
        .work
        .store
        .register_repository(
            &f.work.ctx,
            &other.id,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "fixture/other".into(),
            },
        )
        .unwrap();
    f.sql.execute("UPDATE project_repositories SET remote_id='102',remote_full_name='fixture/other',authorization_status='authorized',connection_id=(SELECT connection_id FROM project_repositories WHERE id=$1) WHERE id=$2", &[&f.repository,&repository.id]).unwrap();
    // Simulate imported history that predates an alias and a failed code analysis.
    f.sql.execute("INSERT INTO git_commit_authorship(id,provider,host,remote_repository_id,repository_id,project_id,commit_sha,author_name,author_email,scope_id,delivery_id) VALUES('gco_history','gitlab','gitlab.com','102',$1,$2,$3,'Git author','global@example.com','fixture','history')", &[&repository.id,&other.id,&"b".repeat(40)]).unwrap();
    // Restore the previous schema constraints so the migration runs against real legacy rows.
    f.sql.batch_execute("ALTER TABLE git_author_aliases DROP CONSTRAINT git_author_alias_organization_scope; DROP INDEX git_author_alias_organization_identity").unwrap();
    for (id, project) in [("gaa_first", &f.work.project.id), ("gaa_second", &other.id)] {
        f.sql.execute("INSERT INTO git_author_aliases(id,actor_id,provider,host,email,project_id,created_by) VALUES($1,$2,'gitlab','gitlab.com','global@example.com',$3,$4)", &[&id,&dev.actor_id(),project,&f.work.ctx.actor_id()]).unwrap();
    }
    f.sql
        .batch_execute(include_str!(
            "../migrations/0089_organization_git_author_aliases.sql"
        ))
        .unwrap();
    let row = f
        .sql
        .query_one(
            "SELECT count(*),count(project_id) FROM git_author_aliases WHERE revoked_at IS NULL",
            &[],
        )
        .unwrap();
    assert_eq!(row.get::<_, i64>(0), 1);
    assert_eq!(row.get::<_, i64>(1), 0);
    let credited: String = f
        .sql
        .query_one(
            "SELECT actor_id FROM git_commit_authorship WHERE id='gco_history'",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(credited, dev.actor_id());
    assert!(
        f.sql
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM developer_rating_dirty WHERE actor_id=$1)",
                &[&dev.actor_id()]
            )
            .unwrap()
            .get::<_, bool>(0)
    );
    // Activity must be visible without an analysis snapshot or binding.
    let profile = f
        .work
        .store
        .project_profile(&f.work.ctx, &other.id, None)
        .unwrap();
    assert!(
        profile["activity"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "commit")
    );
    let profile = f
        .work
        .store
        .member_profile(&f.work.ctx, dev.actor_id(), None)
        .unwrap();
    assert!(
        profile["activity"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "commit")
    );
    let own_profile = f
        .work
        .store
        .member_profile(&dev, dev.actor_id(), None)
        .unwrap();
    assert!(
        own_profile["activity"].as_array().unwrap().is_empty(),
        "Organization-wide authorship must not grant access to another project's code"
    );
    let options = f.work.store.git_author_options(&lead, None).unwrap();
    assert_eq!(options["can_manage_global"], true);
    let future = serde_json::json!({"project_id":101,"object_kind":"push","ref":"refs/heads/develop","after":"c".repeat(40),"commits":[{"id":"c".repeat(40),"author":{"name":"Git author","email":"global@example.com"}}]});
    f.send("future-global", future);
    let credited: String = f
        .sql
        .query_one(
            "SELECT actor_id FROM git_commit_authorship WHERE commit_sha=$1",
            &[&"c".repeat(40)],
        )
        .unwrap()
        .get(0);
    assert_eq!(credited, dev.actor_id());
    // Older clients may send a project, but it must never narrow the stored link.
    let created = f
        .work
        .store
        .create_git_author_alias(
            &lead,
            dev.actor_id(),
            &GitAuthorAliasCreate {
                provider: "gitlab".into(),
                host: "gitlab.com".into(),
                email: "another@example.com".into(),
                project_id: Some(other.id),
                reason: String::new(),
            },
        )
        .unwrap();
    assert!(created["project_id"].is_null());
}

#[test]
#[ignore = "requires PostgreSQL; conflicting legacy identities must never be guessed"]
fn conflicting_legacy_members_stop_the_alias_migration_without_reassigning_work() {
    let mut f = GitAuthorFixture::new();
    let first = f.member("first-author", MemberRole::Developer);
    let second = f.member("second-author", MemberRole::Developer);
    let other = f
        .work
        .store
        .create_project(&f.work.ctx, "Other project", None)
        .unwrap();
    f.sql.batch_execute("ALTER TABLE git_author_aliases DROP CONSTRAINT git_author_alias_organization_scope; DROP INDEX git_author_alias_organization_identity").unwrap();
    for (id, actor, project) in [
        ("gaa_first", first.actor_id(), &f.work.project.id),
        ("gaa_second", second.actor_id(), &other.id),
    ] {
        f.sql.execute("INSERT INTO git_author_aliases(id,actor_id,provider,host,email,project_id,created_by) VALUES($1,$2,'gitlab','gitlab.com','conflict@example.com',$3,$4)", &[&id,&actor,project,&f.work.ctx.actor_id()]).unwrap();
    }
    let error = f
        .sql
        .batch_execute(include_str!(
            "../migrations/0089_organization_git_author_aliases.sql"
        ))
        .unwrap_err();
    assert!(
        error
            .as_db_error()
            .unwrap()
            .message()
            .contains("conflicting Git author members")
    );
    assert_eq!(f.sql.query_one("SELECT count(*) FROM git_author_aliases WHERE revoked_at IS NULL AND project_id IS NOT NULL", &[]).unwrap().get::<_,i64>(0), 2);
}
