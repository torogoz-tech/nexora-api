use super::RepositoryWorkerAnalysisProfiles;
use nexora_platform::RepositoryJobFailure;
use serde_json::json;

#[test]
fn unknown_or_inconsistent_job_policy_is_never_reinterpreted() {
    for (mode, policy) in [
        ("automatic", "manual/v1"),
        ("manual", "automatic-latest/v1"),
        ("automatic", "automatic-latest/v2"),
        ("legacy", "manual/v1"),
    ] {
        assert!(matches!(
            RepositoryWorkerAnalysisProfiles::compile(
                json!({"mode": mode,"policy":policy,"candidates":[]})
            ),
            Err(RepositoryJobFailure::ProfileUnavailable)
        ));
    }
}
#[test]
fn empty_or_malformed_pinned_catalog_fails_before_source_access() {
    for candidates in [json!([]), json!([{"id":"missing-body"}]), json!(null)] {
        assert!(matches!(
            RepositoryWorkerAnalysisProfiles::compile(
                json!({"mode":"automatic","policy":"automatic-latest/v1","candidates":candidates})
            ),
            Err(RepositoryJobFailure::ProfileUnavailable)
        ));
    }
}
