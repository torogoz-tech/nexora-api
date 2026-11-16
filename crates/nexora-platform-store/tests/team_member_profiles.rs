#[path = "support/git_author_fixture.rs"]
mod git_author_fixture;
use git_author_fixture::GitAuthorFixture;
use nexora_platform::*;
#[test]
#[ignore = "requires PostgreSQL"]
fn developer_can_browse_team_profiles_without_administration() {
    let mut f = GitAuthorFixture::new();
    let viewer = f.member("viewer", MemberRole::Developer);
    let peer = f.member("peer", MemberRole::Developer);
    let members = f.work.store.team_members(&viewer).unwrap();
    let entry = members
        .as_array()
        .unwrap()
        .iter()
        .find(|member| member["id"] == peer.actor_id())
        .unwrap();
    assert!(entry.get("email").is_none());
    assert!(entry.get("last_login_at").is_none());
    let profile = f
        .work
        .store
        .member_profile(&viewer, peer.actor_id(), Some(2026))
        .unwrap();
    assert_eq!(profile["member"]["id"], peer.actor_id());
    assert!(profile.get("feedback").is_none());
    assert!(f.work.store.organization_members(&viewer).is_err());
    assert!(matches!(
        f.work.store.member_avatar(&viewer, peer.actor_id()),
        Err(PlatformError::NotFound)
    ));
    f.sql
        .execute(
            "UPDATE actors SET active=false WHERE id=$1",
            &[&peer.actor_id()],
        )
        .unwrap();
    let members = f.work.store.team_members(&viewer).unwrap();
    assert!(
        !members
            .as_array()
            .unwrap()
            .iter()
            .any(|member| member["id"] == peer.actor_id())
    );
    f.sql
        .execute(
            "UPDATE sessions SET revoked_at=now() WHERE id=$1",
            &[&viewer.credential_id()],
        )
        .unwrap();
    assert!(f.work.store.team_members(&viewer).is_err());
    assert!(
        f.work
            .store
            .member_profile(&viewer, peer.actor_id(), Some(2026))
            .is_err()
    );
}

#[test]
#[ignore = "requires PostgreSQL"]
fn management_profiles_do_not_expose_contribution_metrics() {
    let mut f = GitAuthorFixture::new();
    let manager = f.member("manager-chart", MemberRole::Manager);
    let developer = f.member("developer-chart", MemberRole::Developer);
    let task = f.work.task("Recorded work");
    f.sql
        .execute(
            "UPDATE task_history SET actor_id=$1 WHERE task_id=$2",
            &[&manager.actor_id(), &task.id],
        )
        .unwrap();
    let profile = f
        .work
        .store
        .member_profile(&f.work.ctx, manager.actor_id(), Some(2026))
        .unwrap();
    assert_eq!(profile["member"]["role"], "manager");
    assert_eq!(profile["contributions"], serde_json::json!([]));
    // Activity remains readable; only managerial contribution measurement is excluded.
    assert!(!profile["activity"].as_array().unwrap().is_empty());
    f.sql
        .execute(
            "UPDATE task_history SET actor_id=$1 WHERE task_id=$2",
            &[&developer.actor_id(), &task.id],
        )
        .unwrap();
    let profile = f
        .work
        .store
        .member_profile(&f.work.ctx, developer.actor_id(), Some(2026))
        .unwrap();
    assert!(!profile["contributions"].as_array().unwrap().is_empty());
    f.sql
        .execute(
            "UPDATE task_history SET actor_id=$1 WHERE task_id=$2",
            &[&f.work.ctx.actor_id(), &task.id],
        )
        .unwrap();
    let profile = f
        .work
        .store
        .member_profile(&f.work.ctx, f.work.ctx.actor_id(), Some(2026))
        .unwrap();
    assert_eq!(profile["contributions"], serde_json::json!([]));
}
