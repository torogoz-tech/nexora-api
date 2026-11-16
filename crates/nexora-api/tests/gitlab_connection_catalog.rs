#[test]
#[ignore = "requires local PostgreSQL and python3; scoped connection catalog and configuration metadata"]
fn gitlab_connection_pages_and_project_visibility_follow_current_grants() {
    let result = std::process::Command::new("python3")
        .args([
            "tests/support/gitlab_connections.py",
            env!("CARGO_BIN_EXE_nexora-api"),
            "catalog",
        ])
        .output()
        .expect("python3 required");
    assert!(
        result.status.success(),
        "GitLab catalog acceptance failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let evidence: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(evidence["passed"], true);
}
