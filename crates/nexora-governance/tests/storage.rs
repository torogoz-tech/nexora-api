mod support;
use nexora_core::{RepositoryId, TenantId};
use nexora_governance::*;
use support::*;
#[test]
fn result_storage_is_isolated_and_rejects_tampering() {
    let first = evaluate(&snapshot("pass"), policy());
    let mut memory = MemoryFindingRepository::default();
    memory.save(&first).unwrap();
    assert!(
        memory
            .get(&TenantId("other".into()), &first.repository_id, &first.id)
            .unwrap()
            .is_none()
    );
    assert!(
        memory
            .get(&first.tenant_id, &RepositoryId("other".into()), &first.id)
            .unwrap()
            .is_none()
    );
    let dir = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("nexora-gov-store-{}", std::process::id()));
    let mut file = FileFindingRepository::new(&dir).unwrap();
    file.save(&first).unwrap();
    assert_eq!(
        file.get(&first.tenant_id, &first.repository_id, &first.id)
            .unwrap()
            .unwrap()
            .id,
        first.id
    );
    assert_eq!(
        file.latest(&first.tenant_id)
            .unwrap()
            .unwrap()
            .evaluation_id,
        first.id
    );
    assert!(
        file.get(&first.tenant_id, &RepositoryId("other".into()), &first.id)
            .is_err()
    );
    let mut modified = first.clone();
    modified.status = EvaluationStatus::Fail;
    assert!(memory.save(&modified).is_err());
    assert!(
        file.get(&first.tenant_id, &first.repository_id, "../outside")
            .is_err()
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn unknown_is_configurable_without_becoming_a_pass() {
    let snapshot = snapshot("unknown");
    let result = evaluate(&snapshot, policy());
    assert_eq!(result.status, EvaluationStatus::Unknown);
    assert_eq!(GovernanceReporter::exit_code(&result, None), 0);
    assert_eq!(GovernanceReporter::exit_code(&result, Some(true)), 1);
    let mut p = policy();
    p.spec.on_unknown = UnknownBehavior::Fail;
    let result = evaluate(&snapshot, p);
    assert_eq!(GovernanceReporter::exit_code(&result, None), 1);
    assert_eq!(GovernanceReporter::exit_code(&result, Some(false)), 0);
}
