#[test]
#[ignore = "requires PostgreSQL and uv; signed local OIDC provider and real API executable"]
fn corporate_oidc_authenticates_by_issuer_subject_and_keeps_git_and_local_policy_separate() {
    let result = std::process::Command::new("uv")
        .args([
            "run",
            "tests/support/oidc_http.py",
            env!("CARGO_BIN_EXE_nexora-api"),
        ])
        .output()
        .expect("uv is required for opt-in OIDC acceptance");
    assert!(
        result.status.success(),
        "OIDC acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["passed"], true);
}
