#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_platform::*;
use work_item_fixture::WorkItemFixture;
#[test]
#[ignore = "requires PostgreSQL; role scopes, live revocation and future projects"]
fn role_assignment_projects_live_authority_and_prevents_bypasses() {
    let mut f = WorkItemFixture::new();
    assert_eq!(
        f.store.member_role(&f.ctx, f.ctx.actor_id()).unwrap().role,
        Some(MemberRole::SuperAdmin)
    );
    let password =
        zeroize::Zeroizing::new(nexora_platform_store::SecretGenerator::token("password").unwrap());
    let actor = f
        .store
        .create_user(&f.ctx, "member", &password, "Member")
        .unwrap();
    let initial = f.store.member_role(&f.ctx, &actor.id).unwrap();
    assert_eq!(initial.revision, 0);
    let change = MemberRoleChange {
        role: MemberRole::Pmo,
        all_projects: true,
        project_ids: vec![],
        expected_revision: 0,
    };
    let role = f
        .store
        .change_member_role(&f.ctx, &actor.id, &change)
        .unwrap();
    let session = f.store.login("member", &password).unwrap();
    let member = f.store.authenticate(&session.access_token).unwrap();
    let future = f.store.create_project(&f.ctx, "Future", None).unwrap();
    assert!(f.store.get_project(&member, &future.id).is_ok());
    let unrelated_task = f.task("Coordinator visibility");
    assert!(
        f.store
            .get_task(&member, &f.project.id, &unrelated_task.id)
            .is_ok()
    );
    assert!(member.require_human_approval(&future.id).is_ok());
    let broad_key = f
        .store
        .issue_key(
            &f.ctx,
            &actor.id,
            "Role intersection",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![],
                all_projects: true,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let broad_api = f.store.authenticate(&broad_key.key).unwrap();
    assert!(f.store.get_project(&broad_api, &future.id).is_ok());
    let narrow_key = f
        .store
        .issue_key(
            &f.ctx,
            &actor.id,
            "Key intersection",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let narrow_api = f.store.authenticate(&narrow_key.key).unwrap();
    assert!(f.store.get_project(&narrow_api, &future.id).is_err());

    assert!(f.store.member_role(&member, &actor.id).is_err());
    assert!(
        f.store
            .grant(&f.ctx, &actor.id, Capability::InstallationManage, None)
            .is_err()
    );
    assert!(
        f.store
            .change_member_role(&f.ctx, &actor.id, &change)
            .is_err()
    );
    let scoped = MemberRoleChange {
        role: MemberRole::Developer,
        all_projects: false,
        project_ids: vec![f.project.id.clone()],
        expected_revision: role.revision,
    };
    f.store
        .change_member_role(&f.ctx, &actor.id, &scoped)
        .unwrap();
    // The previously authenticated context must not retain removed project authority.
    assert!(f.store.get_project(&member, &future.id).is_err());
    assert!(f.store.get_project(&broad_api, &future.id).is_err());
    assert!(f.store.get_project(&member, &f.project.id).is_ok());
    assert!(
        f.store
            .get_task(&member, &f.project.id, &unrelated_task.id)
            .is_err()
    );
    let refreshed = f.store.authenticate(&session.access_token).unwrap();
    assert!(refreshed.require_human_approval(&f.project.id).is_err());
    let key = f
        .store
        .issue_key(
            &f.ctx,
            &actor.id,
            "Scoped",
            KeyScope {
                capabilities: vec![Capability::ProjectsRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let api = f.store.authenticate(&key.key).unwrap();
    assert!(
        f.store
            .change_member_role(
                &api,
                &actor.id,
                &MemberRoleChange {
                    expected_revision: 2,
                    ..scoped
                }
            )
            .is_err()
    );
    let super_input = MemberRoleChange {
        role: MemberRole::SuperAdmin,
        all_projects: true,
        project_ids: vec![],
        expected_revision: 0,
    };
    assert!(
        f.store
            .change_member_role(&f.ctx, &actor.id, &super_input)
            .is_err()
    );
}

#[test]
#[ignore = "requires PostgreSQL; technical lead scopes and independent positions"]
fn technical_lead_permissions_and_positions_are_live_and_scoped() {
    let mut f = WorkItemFixture::new();
    let password = zeroize::Zeroizing::new(
        nexora_platform_store::SecretGenerator::token("lead_test").unwrap(),
    );
    let actor = f
        .store
        .create_user(&f.ctx, "techlead", &password, "Technical Lead")
        .unwrap();
    let change = MemberRoleChange {
        role: MemberRole::TechnicalLead,
        all_projects: false,
        project_ids: vec![f.project.id.clone()],
        expected_revision: 0,
    };
    f.store
        .change_member_role(&f.ctx, &actor.id, &change)
        .unwrap();
    let session = f.store.login("techlead", &password).unwrap();
    let lead = f.store.authenticate(&session.access_token).unwrap();
    let hidden = f
        .store
        .create_project(&f.ctx, "Outside scope", None)
        .unwrap();
    assert!(f.store.get_project(&lead, &hidden.id).is_err());
    let task = f.task("Other developer work");
    assert!(f.store.get_task(&lead, &f.project.id, &task.id).is_ok());
    let permissions = f.store.effective_permissions(&lead).unwrap();
    assert_eq!(permissions["role"], "technical_lead");
    assert!(
        permissions["grants"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["capability"] == "approvals:grant" && g["project_id"] == f.project.id)
    );
    assert!(
        !permissions["grants"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["capability"] == "installation:manage")
    );
    assert_eq!(
        f.store.feature_catalog(&lead, &f.project.id).unwrap()["can_decide"],
        true
    );
    assert_eq!(f.store.labor_calendar(&lead).unwrap()["can_request"], true);
    assert!(f.store.member_ratings(&lead, &actor.id, 2026).is_ok());
    let input = MemberPositionChange {
        position: " Senior Software Engineer ".into(),
        expected_revision: 0,
    };
    assert!(
        f.store
            .change_member_position(&lead, &actor.id, &input)
            .is_err()
    );
    let saved = f
        .store
        .change_member_position(&f.ctx, &actor.id, &input)
        .unwrap();
    assert_eq!(saved["position"], "Senior Software Engineer");
    assert!(
        f.store
            .change_member_position(&f.ctx, &actor.id, &input)
            .is_err()
    );
    let profile = f.store.member_profile(&lead, &actor.id, None).unwrap();
    assert_eq!(profile["member"]["position"], "Senior Software Engineer");
    assert_eq!(
        f.store.member_role(&f.ctx, &actor.id).unwrap().role,
        Some(MemberRole::TechnicalLead)
    );
    let key = f
        .store
        .issue_key(
            &f.ctx,
            &actor.id,
            "Read-only lead agent",
            KeyScope {
                capabilities: vec![Capability::TasksRead],
                project_ids: vec![f.project.id.clone()],
                all_projects: false,
                installation: false,
            },
            3600,
        )
        .unwrap();
    let api = f.store.authenticate(&key.key).unwrap();
    let attenuated = f.store.effective_permissions(&api).unwrap();
    assert_eq!(attenuated["grants"].as_array().unwrap().len(), 1);
    assert_eq!(attenuated["grants"][0]["capability"], "tasks:read");
    assert!(f.store.feature_catalog(&api, &f.project.id).is_err());
    f.store
        .change_member_role(
            &f.ctx,
            &actor.id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![hidden.id.clone()],
                expected_revision: 1,
            },
        )
        .unwrap();
    assert!(f.store.get_task(&lead, &f.project.id, &task.id).is_err());
    let live = f.store.effective_permissions(&lead).unwrap();
    assert!(
        !live["grants"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["capability"] == "approvals:grant")
    );
}
