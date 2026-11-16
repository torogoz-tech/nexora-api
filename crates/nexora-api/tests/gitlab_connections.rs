#[test]
#[ignore = "requires PostgreSQL and python3; real API and local GitLab OAuth/resource fixture"]
fn gitlab_consent_verifies_remote_access_and_serves_multiple_project_bindings() {
    let result = std::process::Command::new("python3")
        .args([
            "tests/support/gitlab_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
        ])
        .output()
        .expect("python3 required");
    assert!(
        result.status.success(),
        "GitLab acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
}
