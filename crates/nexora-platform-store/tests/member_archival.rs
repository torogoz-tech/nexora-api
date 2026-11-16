#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; archival preserves work and roles but revokes sessions and keys"]
fn archival_is_reversible_without_resurrecting_credentials() {
    let mut f = WorkItemFixture::new();
    let password = nexora_platform_store::SecretGenerator::token("archive_test").unwrap();
    let user = f
        .store
        .create_user(&f.ctx, "archive_user", &password, "Archive user")
        .unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &user.id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: true,
                project_ids: vec![],
                expected_revision: 0,
            },
        )
        .unwrap();
    let directory = f.store.organization_members(&f.ctx).unwrap();
    let before_login = directory
        .as_array()
        .unwrap()
        .iter()
        .find(|member| member["id"] == user.id)
        .unwrap();
    assert!(before_login["last_login_at"].is_null());
    let challenge = f.store.login("archive_user", &password).unwrap();
    let ready = f
        .store
        .identity_gate(&challenge.access_token, "skip", "")
        .unwrap();
    let token = ready["access_token"].as_str().unwrap();
    let user_ctx = f.store.authenticate(token).unwrap();
    let directory = f.store.organization_members(&f.ctx).unwrap();
    let after_login = directory
        .as_array()
        .unwrap()
        .iter()
        .find(|member| member["id"] == user.id)
        .unwrap();
    assert!(after_login["last_login_at"].is_string());
    let key = f
        .store
        .issue_key(
            &f.ctx,
            &user.id,
            "Archive fixture",
            KeyScope {
                capabilities: vec![Capability::TasksRead],
                project_ids: vec![],
                all_projects: true,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let task = f.task("Preserved work");
    assert!(
        f.store
            .change_member_archived(&user_ctx, &user.id, true, 0)
            .is_err()
    );
    let actor = f.ctx.actor_id().to_owned();
    assert!(
        f.store
            .change_member_archived(&f.ctx, &actor, true, 0)
            .is_err()
    );
    let archived = f
        .store
        .change_member_archived(&f.ctx, &user.id, true, 0)
        .unwrap();
    assert_eq!(archived["archived"], true);
    assert_eq!(archived["lifecycle_revision"], 1);
    assert!(f.store.login("archive_user", &password).is_err());
    assert!(f.store.authenticate(token).is_err());
    assert!(f.store.authenticate(&key.key).is_err());
    assert!(
        f.store
            .change_member_archived(&f.ctx, &user.id, false, 0)
            .is_err()
    );
    let restored = f
        .store
        .change_member_archived(&f.ctx, &user.id, false, 1)
        .unwrap();
    assert_eq!(restored["archived"], false);
    assert_eq!(
        f.store.member_role(&f.ctx, &user.id).unwrap().role,
        Some(MemberRole::Developer)
    );
    assert!(f.store.authenticate(token).is_err());
    assert!(f.store.authenticate(&key.key).is_err());
    assert!(f.store.login("archive_user", &password).is_ok());
    assert_eq!(
        f.store
            .get_task(&f.ctx, &f.project.id, &task.id)
            .unwrap()
            .id,
        task.id
    );
}
