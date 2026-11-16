#[path = "support/work_item_fixture.rs"]
mod work_item_fixture;
use nexora_language::AnalysisProfilePackage;
use nexora_platform::*;
use serde_json::json;
use work_item_fixture::WorkItemFixture;
fn package() -> AnalysisProfilePackage {
    AnalysisProfilePackage::from_catalog(
        std::path::Path::new("../../languages/lua"),
        "lua".into(),
        "lua-standard".into(),
        "1.0.0".into(),
        "Lua standard".into(),
    )
    .unwrap()
}
#[test]
#[ignore = "requires PostgreSQL; portable immutable profiles and monotonic assignment"]
fn portable_profile_roundtrip_conflicts_and_monotonic_assignments() {
    let mut f = WorkItemFixture::new();
    let p = package();
    let profile = f.store.import_analysis_profile(&f.ctx, &p).unwrap();
    assert_eq!(
        f.store.import_analysis_profile(&f.ctx, &p).unwrap()["id"],
        profile["id"]
    );
    let exported = f
        .store
        .analysis_profile(&f.ctx, profile["id"].as_str().unwrap(), true)
        .unwrap();
    let exported: AnalysisProfilePackage = serde_json::from_value(exported).unwrap();
    assert_eq!(p.checksum().unwrap(), exported.checksum().unwrap());
    let mut changed = p.clone();
    changed.name = "Changed".into();
    assert!(matches!(
        f.store.import_analysis_profile(&f.ctx, &changed),
        Err(PlatformError::Conflict(_))
    ));
    let repo = f
        .store
        .register_repository(
            &f.ctx,
            &f.project.id,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "fixture/profiles".into(),
            },
        )
        .unwrap();
    let current = f
        .store
        .repository_analysis_profile(&f.ctx, &f.project.id, &repo.id)
        .unwrap();
    assert_eq!(current["revision"], 0);
    let input = RepositoryAnalysisProfileChange {
        expected_revision: 0,
        profile_version_id: profile["id"].as_str().map(str::to_owned),
        reason: "Select model".into(),
        mode: None,
        profile_version_ids: None,
    };
    let selected = f
        .store
        .change_repository_analysis_profile(&f.ctx, &f.project.id, &repo.id, &input)
        .unwrap();
    assert_eq!(selected["revision"], 1);
    let cleared = f
        .store
        .change_repository_analysis_profile(
            &f.ctx,
            &f.project.id,
            &repo.id,
            &RepositoryAnalysisProfileChange {
                expected_revision: 1,
                profile_version_id: None,
                reason: "Use operator fallback".into(),
                mode: None,
                profile_version_ids: None,
            },
        )
        .unwrap();
    assert_eq!(cleared["revision"], 2);
    assert!(cleared["profile"].is_null());
    assert!(
        f.store
            .change_repository_analysis_profile(&f.ctx, &f.project.id, &repo.id, &input)
            .is_err()
    );
    let mut again = input;
    again.expected_revision = 2;
    assert_eq!(
        f.store
            .change_repository_analysis_profile(&f.ctx, &f.project.id, &repo.id, &again)
            .unwrap()["revision"],
        3
    );
}
#[test]
#[ignore = "requires PostgreSQL; least privilege profile administration"]
fn ordinary_members_and_api_keys_cannot_import_engine_models() {
    let mut f = WorkItemFixture::new();
    let pass = zeroize::Zeroizing::new(
        nexora_platform_store::SecretGenerator::token("profile_test").unwrap(),
    );
    let user = f
        .store
        .create_user(&f.ctx, "profile_reader", &pass, "Reader")
        .unwrap();
    f.store
        .change_member_role(
            &f.ctx,
            &user.id,
            &MemberRoleChange {
                role: MemberRole::Developer,
                all_projects: false,
                project_ids: vec![f.project.id.clone()],
                expected_revision: 0,
            },
        )
        .unwrap();
    let session = f.store.login("profile_reader", &pass).unwrap();
    let reader = f.store.authenticate(&session.access_token).unwrap();
    assert!(matches!(
        f.store.import_analysis_profile(&reader, &package()),
        Err(PlatformError::Forbidden)
    ));
    let key = f
        .store
        .issue_key(
            &f.ctx,
            f.ctx.actor_id(),
            "No human config impersonation",
            KeyScope {
                capabilities: Capability::all(),
                project_ids: vec![],
                all_projects: true,
                installation: true,
            },
            3600,
        )
        .unwrap();
    let agent = f.store.authenticate(&key.key).unwrap();
    assert!(matches!(
        f.store.import_analysis_profile(&agent, &package()),
        Err(PlatformError::Forbidden)
    ));
    assert!(f.store.analysis_profiles(&reader, None, 10).is_err());
    let repo = f
        .store
        .register_repository(
            &f.ctx,
            &f.project.id,
            &RepositoryRegistration {
                provider: GitProvider::Gitlab,
                full_name: "fixture/reader".into(),
            },
        )
        .unwrap();
    let selected = f
        .store
        .repository_analysis_profile(&reader, &f.project.id, &repo.id)
        .unwrap();
    assert_eq!(selected["can_manage"], json!(false));
}
