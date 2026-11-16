#[allow(dead_code)]
#[path = "support/task_pool_fixture.rs"]
mod task_pool_fixture;
use nexora_platform::*;
use nexora_platform_store::{BranchProtectionApply, BranchProtectionPreview, GitNetworkPolicy};
use task_pool_fixture::TaskPoolFixture;
#[test]
#[ignore = "requires PostgreSQL"]
fn branch_protection_is_readable_but_only_human_managers_can_prepare_or_apply() {
    let mut f = TaskPoolFixture::new(false);
    let project = f.work.project.id.clone();
    let repository = f
        .work
        .store
        .register_repository(
            &f.work.ctx,
            &project,
            &RepositoryRegistration {
                provider: GitProvider::Github,
                full_name: "fixture/protected".into(),
            },
        )
        .unwrap();
    let input = BranchProtectionPreview {
        required_checks: vec!["test".into()],
    };
    let policy = GitNetworkPolicy::default();
    for ctx in [&f.developer, &f.other] {
        let record = f
            .work
            .store
            .repository_branch_protection(ctx, &project, &repository.id)
            .unwrap();
        assert_eq!(record["status"], "unconfigured");
        assert!(matches!(
            f.work.store.preview_repository_branch_protection(
                ctx,
                &project,
                &repository.id,
                &input,
                &policy
            ),
            Err(PlatformError::Forbidden)
        ));
        assert!(matches!(
            f.work.store.apply_repository_branch_protection(
                ctx,
                &project,
                &repository.id,
                &BranchProtectionApply {
                    expected_revision: 1
                },
                &policy
            ),
            Err(PlatformError::Forbidden)
        ));
    }
    // These fail locally before any provider request, even for an administrator.
    assert!(matches!(
        f.work.store.preview_repository_branch_protection(
            &f.work.ctx,
            &project,
            &repository.id,
            &BranchProtectionPreview {
                required_checks: vec!["test\nmalformed".into()]
            },
            &policy
        ),
        Err(PlatformError::Validation(_))
    ));
    assert!(matches!(
        f.work.store.apply_repository_branch_protection(
            &f.work.ctx,
            &project,
            &repository.id,
            &BranchProtectionApply {
                expected_revision: 0
            },
            &policy
        ),
        Err(PlatformError::Validation(_))
    ));
    assert!(matches!(
        f.work.store.apply_repository_branch_protection(
            &f.manager,
            &project,
            &repository.id,
            &BranchProtectionApply {
                expected_revision: 1
            },
            &policy
        ),
        Err(PlatformError::NotFound)
    ));
}
