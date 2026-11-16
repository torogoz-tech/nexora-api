#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; global manager configuration, MCP attenuation and immediate demotion"]
fn manager_configuration_is_global_without_changing_operational_project_scope() {
    let mut f = WorkItemFixture::new();
    let password = nexora_platform_store::SecretGenerator::token("manager_test").unwrap();
    let actor = f
        .store
        .create_user(&f.ctx, "manager_test", &password, "Manager")
        .unwrap();
    let assignment = f
        .store
        .change_member_role(
            &f.ctx,
            &actor.id,
            &MemberRoleChange {
                role: MemberRole::Manager,
                all_projects: false,
                project_ids: vec![f.project.id.clone()],
                expected_revision: 0,
            },
        )
        .unwrap();
    let challenge = f.store.login("manager_test", &password).unwrap();
    let ready = f
        .store
        .identity_gate(&challenge.access_token, "skip", "")
        .unwrap();
    let manager = f
        .store
        .authenticate(ready["access_token"].as_str().unwrap())
        .unwrap();
    for cap in MemberRole::Manager.configuration_capabilities() {
        manager.require(cap, None).unwrap();
    }
    assert!(
        manager
            .require(Capability::TasksRead, Some(&f.project.id))
            .is_ok()
    );
    assert!(
        manager
            .require(Capability::TasksRead, Some("unassigned_project"))
            .is_err()
    );
    assert!(
        manager
            .require(Capability::DocumentsWrite, Some("unassigned_project"))
            .is_err()
    );
    assert!(f.store.organization_members(&manager).is_ok());
    assert!(f.store.git_connections(&manager).is_ok());
    assert!(f.store.oidc_providers(&manager).is_ok());
    assert!(f.store.email_access_settings(&manager).is_ok());
    assert!(
        f.store
            .set_totp_exemption(&manager, &actor.id, true)
            .is_err()
    );
    let key = f
        .store
        .issue_key(
            &f.ctx,
            &actor.id,
            "Restricted MCP",
            KeyScope {
                capabilities: vec![Capability::RepositoriesRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let api = f.store.authenticate(&key.key).unwrap();
    assert!(f.store.git_connections(&api).is_err());
    assert!(f.store.email_access_settings(&api).is_err());
    f.store
        .change_member_role(
            &f.ctx,
            &actor.id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![f.project.id.clone()],
                expected_revision: assignment.revision,
            },
        )
        .unwrap();
    assert!(f.store.git_connections(&manager).is_err());
    assert!(f.store.email_access_settings(&manager).is_err());
}
